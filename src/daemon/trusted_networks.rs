//! Trusted networks watcher for PrivadoVPN.
//!
//! Monitors WiFi SSID changes via NetworkManager's nmcli tool. When the
//! connected WiFi network is in the user's trusted_networks list, the VPN
//! auto-disconnects. When leaving a trusted network, auto-connects if the
//! auto_connect config option is enabled.
//!
//! Runs as a background tokio task in the daemon, polling every 10 seconds.

use crate::daemon::state::SharedState;
use crate::daemon::{http_api, swanctl};
use tokio::process::Command;
use tracing::{info, error};

/// Polling interval for SSID changes.
const POLL_INTERVAL_SECS: u64 = 10;

/// Main watcher loop. Runs forever, checking WiFi SSID against the trusted
/// networks list and taking action when transitions occur.
pub async fn run_watcher(state: SharedState) {
    info!("[trusted-networks] watcher starting");

    let mut last_ssid: Option<String> = None;
    let mut was_trusted = false;

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;

        let cfg = match crate::config::load_config() {
            Some(c) => c,
            None => continue,
        };

        // Get current WiFi SSID up front so the transition-tracking state
        // (`last_ssid` / `was_trusted`) is advanced exactly ONCE per iteration,
        // at the bottom of the loop, regardless of which branch runs. This
        // prevents a stale `last_ssid` from masking a real change after an
        // early `continue`.
        let current_ssid = get_current_ssid().await;

        let is_trusted = match &current_ssid {
            Some(ssid) => cfg.trusted_networks.iter().any(|t| t.eq_ignore_ascii_case(ssid)),
            None => false,
        };

        // Skip action if no trusted networks configured, but STILL advance the
        // baseline so re-enabling trusted networks (possibly while already on
        // one) starts from a correct `was_trusted=false` / current `last_ssid`.
        if cfg.trusted_networks.is_empty() {
            was_trusted = false;
            last_ssid = current_ssid;
            continue;
        }

        // Detect SSID change.
        if current_ssid == last_ssid {
            continue;
        }

        let prev_ssid = last_ssid.clone();
        let ssid_display = current_ssid.as_deref().unwrap_or("<disconnected>");

        // When a reconnect attempt fails we want to retry next tick WITHOUT
        // relying on a stale SSID to re-fire the change detector. Advancing
        // `last_ssid` (always, at the bottom) plus holding `was_trusted` at its
        // previous value drives the retry intentionally.
        let mut connect_failed = false;

        if is_trusted && !was_trusted {
            // Entering a trusted network — auto-disconnect the VPN.
            info!("[trusted-networks] entered trusted network '{ssid_display}', disconnecting VPN");

            // Capture endpoint IPs before revoke() clears them (R5).
            let remote_ips = {
                let mut st = state.write().await;
                let ips = st.current_remote_ips.clone();
                st.revoke();
                ips
            };
            let _ = tokio::task::spawn_blocking(move || {
                crate::routing::on_disconnect(&remote_ips);
                crate::daemon::routing_rules::clear_routing_rules();
            }).await;
            swanctl::terminate_all_privado().await;
            swanctl::cleanup_config().await;
        } else if !is_trusted && was_trusted && cfg.auto_connect {
            // Left a trusted network with auto_connect enabled — reconnect.
            info!("[trusted-networks] left trusted network, auto-connecting VPN");

            if !cfg.username.is_empty() && !cfg.password.is_empty() {
                let country = cfg.preferred_country.clone().unwrap_or_else(|| "nl".to_string());
                let server_host = http_api::country_to_default_host(&country);

                if let Err(e) = swanctl::ensure_strongswan_up().await {
                    error!("[trusted-networks] strongSwan start failed: {e}");
                    connect_failed = true;
                } else {
                    let routes = if cfg.split_tunnel && !cfg.split_domains.is_empty() {
                        crate::routing::generate_split_routes(&cfg.split_domains)
                    } else {
                        vec!["0.0.0.0/0".to_string()]
                    };

                    if let Err(e) = swanctl::write_dynamic_config(
                        &server_host, &cfg.username, &cfg.password, &routes, &cfg.dns_servers,
                    ).await {
                        error!("[trusted-networks] config write failed: {e}");
                        connect_failed = true;
                    } else {
                        let cc = http_api::derive_country_from_host(&server_host);
                        {
                            let mut st = state.write().await;
                            st.authorize(cc);
                            st.current_server = Some(server_host.clone());
                        }

                        if let Err(e) = swanctl::initiate_dynamic().await {
                            error!("[trusted-networks] initiate failed: {e}");
                            state.write().await.revoke();
                            connect_failed = true;
                        } else {
                            let dns = cfg.dns_servers.clone();
                            let ks = cfg.kill_switch;
                            let sd = cfg.split_domains.clone();
                            let cfg_for_rules = cfg.clone();
                            let remote_ips = crate::routing::resolve_domain_ips(std::slice::from_ref(&server_host));
                            state.write().await.current_remote_ips = remote_ips.clone();
                            tokio::task::spawn_blocking(move || {
                                crate::routing::on_connect(&remote_ips, &dns, ks, &sd);
                                crate::daemon::routing_rules::apply_routing_rules(&cfg_for_rules);
                            });

                            info!("[trusted-networks] reconnected to {server_host}");
                        }
                    }
                }
            }
        } else if current_ssid != prev_ssid {
            info!("[trusted-networks] SSID changed to '{ssid_display}' (trusted={is_trusted})");
        }

        // Single, bottom-of-loop state advance. `last_ssid` always tracks the
        // observed SSID so the change detector stays in sync. `was_trusted` is
        // held at its prior value on a failed reconnect so the next tick retries
        // the connect; otherwise it follows the current trust state.
        last_ssid = current_ssid;
        if !connect_failed {
            was_trusted = is_trusted;
        }
    }
}

/// Get the currently connected WiFi SSID.
/// Tries multiple methods in order: nmcli → iw → wpa_cli → /proc/net/wireless.
/// Returns None if not connected to WiFi or no method works.
async fn get_current_ssid() -> Option<String> {
    // Method 1: nmcli (NetworkManager, most common on desktop Linux).
    if let Some(ssid) = get_ssid_nmcli().await {
        return Some(ssid);
    }

    // Method 2: iw (works with any wireless driver, no NM needed).
    if let Some(ssid) = get_ssid_iw().await {
        return Some(ssid);
    }

    // Method 3: wpa_cli (wpa_supplicant direct, common on minimal installs).
    if let Some(ssid) = get_ssid_wpa_cli().await {
        return Some(ssid);
    }

    None
}

async fn get_ssid_nmcli() -> Option<String> {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "active,ssid", "dev", "wifi"])
        .output().await.ok()?;

    if !output.status.success() { return None; }

    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(ssid) = line.strip_prefix("yes:") {
            let ssid = ssid.trim();
            if !ssid.is_empty() {
                return Some(ssid.to_string());
            }
        }
    }
    None
}

async fn get_ssid_iw() -> Option<String> {
    // First detect the wireless interface.
    let iface = detect_wifi_interface().await?;

    // iw dev <iface> link → shows SSID if connected.
    let output = Command::new("iw")
        .args(["dev", &iface, "link"])
        .output().await.ok()?;

    if !output.status.success() { return None; }

    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(ssid) = trimmed.strip_prefix("SSID:") {
            let ssid = ssid.trim();
            if !ssid.is_empty() {
                return Some(ssid.to_string());
            }
        }
    }
    None
}

async fn get_ssid_wpa_cli() -> Option<String> {
    // wpa_cli status → output contains "ssid=NetworkName"
    let output = Command::new("wpa_cli")
        .args(["status"])
        .output().await.ok()?;

    if !output.status.success() { return None; }

    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(ssid) = line.strip_prefix("ssid=") {
            let ssid = ssid.trim();
            if !ssid.is_empty() {
                return Some(ssid.to_string());
            }
        }
    }
    None
}

/// Detect the primary wireless interface name.
async fn detect_wifi_interface() -> Option<String> {
    let output = Command::new("ip")
        .args(["-o", "link", "show"])
        .output().await.ok()?;

    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let iface = parts[1].trim_end_matches(':');
            if iface.starts_with("wl") {
                return Some(iface.to_string());
            }
        }
    }
    None
}

/// List available WiFi networks (for UI display in trusted networks settings).
/// Exposed for the settings UI's network picker; not yet called from the
/// daemon's own paths, so allow it until the picker endpoint is wired.
#[allow(dead_code)]
pub async fn list_available_networks() -> Vec<String> {
    let output = match Command::new("nmcli")
        .args(["-t", "-f", "ssid", "dev", "wifi", "list"])
        .output().await
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut networks: Vec<String> = text.lines()
        .map(|l| l.trim().to_string())
        .filter(|s| !s.is_empty() && s != "--")
        .collect();

    networks.sort();
    networks.dedup();
    networks
}
