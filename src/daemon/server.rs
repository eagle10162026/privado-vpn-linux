//! Unix-socket request handler. Listens on `SOCKET_PATH` and serves
//! Request → Response one-shot exchanges. The socket is chmod 0666 so any
//! local user can issue a request; state-changing operations are gated by
//! the PIN check below.

use crate::daemon::proto::{ErrorCode, Request, Response, ProvisionedServer, SOCKET_PATH};
use crate::daemon::state::SharedState;
use crate::daemon::swanctl;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::{info, warn};

/// PIN required for connect/disconnect operations.
/// Set via PRIVADO_VPN_PIN env var or defaults to "1234".
pub fn vpn_pin() -> &'static str {
    static PIN: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PIN.get_or_init(|| {
        std::env::var("PRIVADO_VPN_PIN").unwrap_or_else(|_| "1234".to_string())
    })
}

pub async fn run_server(state: SharedState) -> Result<(), String> {
    let _ = std::fs::remove_file(SOCKET_PATH);
    let listener = UnixListener::bind(SOCKET_PATH)
        .map_err(|e| format!("bind {SOCKET_PATH}: {e}"))?;
    std::fs::set_permissions(SOCKET_PATH, std::fs::Permissions::from_mode(0o666))
        .map_err(|e| format!("chmod {SOCKET_PATH}: {e}"))?;
    info!("[server] listening on {SOCKET_PATH}");
    loop {
        let (stream, _addr) = listener.accept().await
            .map_err(|e| format!("accept: {e}"))?;
        let st = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_conn(stream, st).await {
                warn!("[server] connection error: {e}");
            }
        });
    }
}

async fn handle_conn(stream: UnixStream, state: SharedState) -> Result<(), String> {
    let (rd, mut wr) = stream.into_split();
    let mut reader = BufReader::new(rd);
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await
            .map_err(|e| format!("read: {e}"))?;
        if n == 0 { return Ok(()); }

        let req: Request = match serde_json::from_str(line.trim()) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response::Err {
                    code: ErrorCode::BadRequest,
                    message: format!("malformed JSON: {e}"),
                };
                write_response(&mut wr, &resp).await?;
                continue;
            }
        };
        let resp = dispatch(req, &state).await;
        write_response(&mut wr, &resp).await?;
    }
}

async fn write_response(
    wr: &mut tokio::net::unix::OwnedWriteHalf,
    resp: &Response,
) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(resp).map_err(|e| format!("serialize: {e}"))?;
    bytes.push(b'\n');
    wr.write_all(&bytes).await.map_err(|e| format!("write: {e}"))?;
    Ok(())
}

async fn dispatch(req: Request, state: &SharedState) -> Response {
    match req {
        Request::Status => {
            let mut status = swanctl::live_status().await;
            let snap = state.read().await;
            status.authorized    = snap.authorization_fresh();
            status.authorized_at = snap.authorized_at.map(format_iso_utc);
            status.country       = snap.country.clone();
            Response::Ok { status }
        }
        Request::Servers => {
            // Fetch full server list from Portal API (same as HTTP /servers endpoint).
            let portal_servers = crate::daemon::portal_api::get_servers().await;
            let entries: Vec<ProvisionedServer> = portal_servers.iter()
                .map(|s| ProvisionedServer {
                    country_code: s.country_code.to_lowercase(),
                    display: if s.city.is_empty() {
                        format!("{} ({})", s.country, s.country_code)
                    } else {
                        format!("{}, {} ({})", s.city, s.country, s.country_code)
                    },
                    remote_host: s.hostname.clone(),
                })
                .collect();
            Response::Servers { entries }
        }
        Request::Connect { pin, country } => {
            if pin != vpn_pin() {
                return Response::Err { code: ErrorCode::BadPin, message: "PIN rejected".into() };
            }
            // Look up best server for the given country from the Portal API.
            let host_owned = match crate::daemon::portal_api::find_server_for_country(&country).await {
                Some(server) => server.hostname,
                None => crate::daemon::http_api::country_to_default_host(&country),
            };
            let host = host_owned.as_str();

            let cfg = crate::config::load_config().unwrap_or_default();
            if cfg.username.is_empty() || cfg.password.is_empty() {
                return Response::Err {
                    code: ErrorCode::BadRequest,
                    message: "No credentials. Run `privado-vpn login` first.".into(),
                };
            }

            if let Err(e) = swanctl::ensure_strongswan_up().await {
                return Response::Err { code: ErrorCode::StrongswanError, message: e };
            }

            let routes = if cfg.split_tunnel && !cfg.split_domains.is_empty() {
                crate::routing::generate_split_routes(&cfg.split_domains)
            } else {
                vec!["0.0.0.0/0".to_string()]
            };

            if let Err(e) = swanctl::write_dynamic_config(host, &cfg.username, &cfg.password, &routes, &cfg.dns_servers).await {
                return Response::Err { code: ErrorCode::StrongswanError, message: e };
            }

            {
                let mut st = state.write().await;
                st.authorize(country.clone());
                st.current_server = Some(host_owned.clone());
            }

            if let Err(e) = swanctl::initiate_dynamic().await {
                state.write().await.revoke();
                return Response::Err { code: ErrorCode::StrongswanError, message: e };
            }

            // Install routing. Resolve + persist the endpoint IPs (R5) so a later
            // disconnect removes exactly these mangle MARK rules, then apply the
            // routing-rule engine.
            let dns = cfg.dns_servers.clone();
            let ks = cfg.kill_switch;
            let sd = cfg.split_domains.clone();
            let cfg_for_rules = cfg.clone();
            let remote_ips = crate::routing::resolve_domain_ips(std::slice::from_ref(&host_owned));
            state.write().await.current_remote_ips = remote_ips.clone();
            tokio::task::spawn_blocking(move || {
                crate::routing::on_connect(&remote_ips, &dns, ks, &sd);
                crate::daemon::routing_rules::apply_routing_rules(&cfg_for_rules);
            });

            tokio::time::sleep(Duration::from_millis(700)).await;

            let mut status = swanctl::live_status().await;
            let snap = state.read().await;
            status.authorized    = snap.authorization_fresh();
            status.authorized_at = snap.authorized_at.map(format_iso_utc);
            status.country       = snap.country.clone();
            Response::Ok { status }
        }
        Request::Disconnect { pin } => {
            if pin != vpn_pin() {
                return Response::Err { code: ErrorCode::BadPin, message: "PIN rejected".into() };
            }
            // Capture the resolved endpoint IPs before revoke() clears them (R5).
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

            tokio::time::sleep(Duration::from_millis(300)).await;
            let status = swanctl::live_status().await;
            Response::Ok { status }
        }
    }
}

fn format_iso_utc(t: SystemTime) -> String {
    let dur = t.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    let secs = dur.as_secs();
    let (year, month, day, hour, minute, second) = ymd_hms_from_unix(secs as i64);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn ymd_hms_from_unix(t: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = t.div_euclid(86_400);
    let rem  = t.rem_euclid(86_400) as u32;
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    let second = rem % 60;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y   = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp  = (5 * doy + 2) / 153;
    let d   = doy - (153 * mp + 2) / 5 + 1;
    let m   = if mp < 10 { mp + 3 } else { mp - 9 };
    let y   = if m <= 2 { y + 1 } else { y };
    (y, m, d, hour, minute, second)
}
