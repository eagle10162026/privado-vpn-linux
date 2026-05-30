//! Additional daemon HTTP API handlers: GeoJump reconnect and pause connection.

use axum::{extract::State, http::StatusCode, Json};
use std::time::Duration;
use tracing::{info, error};

use crate::daemon::proto::{ErrorCode, Response};
use crate::daemon::state::SharedState;
use crate::daemon::swanctl;
use crate::daemon::server::vpn_pin;
use crate::daemon::http_api::{country_to_default_host, derive_country_from_host};

use std::time::SystemTime;

fn format_iso_utc(t: SystemTime) -> String {
    use std::time::UNIX_EPOCH;
    let dur = t.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    let secs = dur.as_secs();
    let days = (secs / 86400) as i64;
    let rem = (secs % 86400) as u32;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, (rem % 3600) / 60, rem % 60)
}

/// GeoJump reconnect: terminates the current SA and initiates a new one
/// to the target server in a single atomic operation.
#[derive(serde::Deserialize)]
pub struct ReconnectReq {
    pub pin: String,
    pub server_host: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub routes: Vec<String>,
    #[serde(default)]
    pub dns: Vec<String>,
    #[serde(default)]
    pub kill_switch: Option<bool>,
}

pub async fn handle_reconnect(
    State(state): State<SharedState>,
    Json(body): Json<ReconnectReq>,
) -> (StatusCode, Json<Response>) {
    if body.pin != vpn_pin() {
        return (StatusCode::FORBIDDEN, Json(Response::Err {
            code: ErrorCode::BadPin, message: "PIN rejected".into(),
        }));
    }
    if body.server_host.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(Response::Err {
            code: ErrorCode::BadRequest, message: "server_host required".into(),
        }));
    }

    let cfg = crate::config::load_config().unwrap_or_default();
    let username = body.username.filter(|s| !s.is_empty()).unwrap_or_else(|| cfg.username.clone());
    let password = body.password.filter(|s| !s.is_empty()).unwrap_or_else(|| cfg.password.clone());
    if username.is_empty() || password.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(Response::Err {
            code: ErrorCode::BadRequest, message: "No credentials".into(),
        }));
    }

    let routes = if body.routes.is_empty() {
        if cfg.split_tunnel && !cfg.split_domains.is_empty() {
            crate::routing::generate_split_routes(&cfg.split_domains)
        } else {
            vec!["0.0.0.0/0".to_string()]
        }
    } else {
        body.routes
    };
    let dns = if body.dns.is_empty() { cfg.dns_servers.clone() } else { body.dns };
    let kill_switch = body.kill_switch.unwrap_or(cfg.kill_switch);

    // Terminate existing SA without tearing down full routing.
    swanctl::terminate_all_privado().await;

    // Write new config for target server.
    if let Err(e) = swanctl::write_dynamic_config(
        &body.server_host, &username, &password, &routes, &dns,
    ).await {
        state.write().await.revoke();
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(Response::Err {
            code: ErrorCode::StrongswanError, message: e,
        }));
    }

    let country_code = derive_country_from_host(&body.server_host);
    state.write().await.authorize(country_code);

    // Initiate new connection.
    if let Err(e) = swanctl::initiate_dynamic().await {
        state.write().await.revoke();
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(Response::Err {
            code: ErrorCode::StrongswanError, message: e,
        }));
    }

    // Reinstall routing for new server.
    let server_host_owned = body.server_host.clone();
    let split_domains = cfg.split_domains.clone();
    let dns_c = dns.clone();
    tokio::task::spawn_blocking(move || {
        let remote_ips = crate::routing::resolve_domain_ips(&[server_host_owned]);
        crate::routing::on_connect(&remote_ips, &dns_c, kill_switch, &split_domains);
    });

    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut status = swanctl::live_status().await;
    let snap = state.read().await;
    status.authorized = snap.authorization_fresh();
    status.authorized_at = snap.authorized_at.map(format_iso_utc);
    status.country = snap.country.clone();
    (StatusCode::OK, Json(Response::Ok { status }))
}

/// Pause the VPN for a given duration, then auto-reconnect.
#[derive(serde::Deserialize)]
pub struct PauseReq {
    pub pin: String,
    pub duration_secs: u64,
}

pub async fn handle_pause(
    State(state): State<SharedState>,
    Json(body): Json<PauseReq>,
) -> (StatusCode, Json<Response>) {
    if body.pin != vpn_pin() {
        return (StatusCode::FORBIDDEN, Json(Response::Err {
            code: ErrorCode::BadPin, message: "PIN rejected".into(),
        }));
    }
    if body.duration_secs == 0 || body.duration_secs > 86400 {
        return (StatusCode::BAD_REQUEST, Json(Response::Err {
            code: ErrorCode::BadRequest, message: "duration_secs must be 1..86400".into(),
        }));
    }

    let current_status = swanctl::live_status().await;
    if !current_status.connected {
        return (StatusCode::BAD_REQUEST, Json(Response::Err {
            code: ErrorCode::BadRequest, message: "Not connected".into(),
        }));
    }

    // Terminate SA and remove routing while paused.
    swanctl::terminate_all_privado().await;
    let _ = tokio::task::spawn_blocking(|| {
        crate::routing::on_disconnect(&[]);
    }).await;

    // Record the pause in daemon state so other paths (trusted-network
    // auto-connect, a manual reconnect) can tell we're intentionally paused,
    // and so the resume timer can detect if the pause was cancelled out from
    // under it instead of blindly reconnecting.
    state.write().await.set_paused(body.duration_secs);
    info!("[pause] VPN paused for {} seconds", body.duration_secs);

    // Background task: wait, then reconnect.
    let pause_state = state.clone();
    let pause_duration = body.duration_secs;
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(pause_duration)).await;
        // If the pause was cancelled during the window (e.g. the user manually
        // reconnected — authorize() clears `paused`), don't clobber that state.
        if !pause_state.read().await.is_paused() {
            info!("[pause] pause cancelled before timer fired — skipping auto-resume");
            return;
        }
        info!("[pause] resuming connection");

        let cfg = match crate::config::load_config() {
            Some(c) => c,
            None => { error!("[pause] no config for resume"); return; }
        };
        if cfg.username.is_empty() || cfg.password.is_empty() {
            error!("[pause] no credentials for resume");
            return;
        }

        let country = cfg.preferred_country.clone().unwrap_or_else(|| "nl".to_string());
        let server_host = country_to_default_host(&country);
        let routes = if cfg.split_tunnel && !cfg.split_domains.is_empty() {
            crate::routing::generate_split_routes(&cfg.split_domains)
        } else {
            vec!["0.0.0.0/0".to_string()]
        };

        if swanctl::write_dynamic_config(
            &server_host, &cfg.username, &cfg.password, &routes, &cfg.dns_servers,
        ).await.is_err() {
            pause_state.write().await.revoke();
            return;
        }

        let cc = derive_country_from_host(&server_host);
        pause_state.write().await.authorize(cc);

        if swanctl::initiate_dynamic().await.is_err() {
            pause_state.write().await.revoke();
            return;
        }

        let dns = cfg.dns_servers.clone();
        let ks = cfg.kill_switch;
        let sd = cfg.split_domains.clone();
        let sh = server_host;
        tokio::task::spawn_blocking(move || {
            let ips = crate::routing::resolve_domain_ips(&[sh]);
            crate::routing::on_connect(&ips, &dns, ks, &sd);
        });

        info!("[pause] VPN resumed");
    });

    (StatusCode::OK, Json(Response::Ok { status: current_status }))
}
