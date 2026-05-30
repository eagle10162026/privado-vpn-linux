//! In-process backend daemon. One per host. Owns the swanctl lifecycle,
//! the authorization marker, the guardian loop, and the IPC socket that
//! the CLI subcommands and the GTK GUI talk to.
//!
//! Started by `privado-vpn daemon` (run via the systemd unit
//! `privado-vpn.service`). Replaces the former
//! `/usr/local/bin/vpn-control` + `vpn-guardian.timer` glue — those files
//! are removed at install time.

pub mod proto;
pub mod state;
pub mod swanctl;
mod guardian;
pub mod server;
pub mod http_api;
pub mod extra_handlers;
pub mod portal_api;
pub mod wireguard;
pub mod openvpn;
pub mod trusted_networks;

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Run the daemon forever. Returns only on fatal IO error.
pub async fn run() -> Result<(), String> {
    info!("privado-vpn daemon starting (pid {})", std::process::id());

    let shared = Arc::new(RwLock::new(state::DaemonState::default()));

    // Tear down any existing privado-* SAs from a previous boot so the
    // first user-driven connect starts from a clean slate. The guardian
    // would do this on its first tick anyway, but doing it eagerly avoids
    // a 30-second window of "looks connected but isn't authorized."
    swanctl::terminate_all_privado().await;

    // Remove stale legacy per-country confs (catch-all remote_ts=0.0.0.0/0 with
    // NO mark_out). If `swanctl --load-all` ever loaded one and it got
    // initiated, it would capture ALL traffic (LAN + paired S22 + forwarded)
    // into the tunnel and kill the host network stack. Only the dynamic
    // privado.conf (mark_out=0x1016 split-tunnel) is valid.
    swanctl::purge_stale_confs().await;

    // Spawn the guardian.
    let g = shared.clone();
    tokio::spawn(async move { guardian::run_loop(g).await; });

    // Spawn the HTTP API (127.10.0.18:1600 + :1601 health).
    let h = shared.clone();
    tokio::spawn(async move {
        if let Err(e) = http_api::run_http_api(h).await {
            error!("http-api exited: {e}");
        }
    });

    // Spawn the trusted networks watcher (monitors WiFi SSID changes via
    // NetworkManager D-Bus and auto-connects/disconnects based on config).
    let tn = shared.clone();
    tokio::spawn(async move {
        trusted_networks::run_watcher(tn).await;
    });

    // Auto-connect on boot: ONLY if the user has explicitly enabled auto_connect
    // in their config AND has saved credentials. The VPN NEVER connects on its own
    // unless the user configured it or tells the LLM/CLI to connect.
    let ac = shared.clone();
    tokio::spawn(async move {
        auto_connect_on_boot(ac).await;
    });

    // Run the Unix socket server until shutdown.
    if let Err(e) = server::run_server(shared).await {
        error!("server exited: {e}");
        return Err(e);
    }
    Ok(())
}

/// Waits 10 seconds for the network to settle after boot, then connects
/// using the user's preferred country if auto_connect is enabled.
async fn auto_connect_on_boot(state: state::SharedState) {
    // Give the system time to bring up the network interface and obtain
    // a DHCP lease before attempting to connect through the VPN.
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    let cfg = match crate::config::load_config() {
        Some(c) => c,
        None => { info!("[auto-connect] no config found, skipping"); return; }
    };

    if !cfg.auto_connect {
        info!("[auto-connect] disabled in config, skipping");
        return;
    }

    if cfg.username.is_empty() || cfg.password.is_empty() {
        warn!("[auto-connect] auto_connect enabled but no credentials saved");
        return;
    }

    // Verify we have network connectivity before trying to connect.
    let has_network = tokio::process::Command::new("ip")
        .args(["route", "show", "default"])
        .output().await
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);

    if !has_network {
        warn!("[auto-connect] no default route found, waiting 10 more seconds...");
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        let retry = tokio::process::Command::new("ip")
            .args(["route", "show", "default"])
            .output().await
            .map(|o| !o.stdout.is_empty())
            .unwrap_or(false);
        if !retry {
            error!("[auto-connect] still no network after 20s, giving up");
            return;
        }
    }

    info!("[auto-connect] triggering connection for user {}", cfg.username);

    let country = cfg.preferred_country.clone().unwrap_or_else(|| "nl".to_string());
    let server_host = http_api::country_to_default_host(&country);

    // Ensure strongSwan is running.
    if let Err(e) = swanctl::ensure_strongswan_up().await {
        error!("[auto-connect] strongSwan start failed: {e}");
        return;
    }

    // Write config and initiate.
    let routes = if cfg.split_tunnel && !cfg.split_domains.is_empty() {
        crate::routing::generate_split_routes(&cfg.split_domains)
    } else {
        vec!["0.0.0.0/0".to_string()]
    };

    if let Err(e) = swanctl::write_dynamic_config(
        &server_host, &cfg.username, &cfg.password, &routes, &cfg.dns_servers,
    ).await {
        error!("[auto-connect] write config failed: {e}");
        return;
    }

    // Authorize in daemon state.
    {
        let cc = http_api::derive_country_from_host(&server_host);
        state.write().await.authorize(cc);
    }

    if let Err(e) = swanctl::initiate_dynamic().await {
        error!("[auto-connect] initiate failed: {e}");
        state.write().await.revoke();
        return;
    }

    // Install routing.
    let dns = cfg.dns_servers.clone();
    let kill_switch = cfg.kill_switch;
    let split_domains = cfg.split_domains.clone();
    let server_host_owned = server_host.clone();
    tokio::task::spawn_blocking(move || {
        let remote_ips = crate::routing::resolve_domain_ips(&[server_host_owned]);
        crate::routing::on_connect(&remote_ips, &dns, kill_switch, &split_domains);
    });

    info!("[auto-connect] connection initiated to {server_host}");
}
