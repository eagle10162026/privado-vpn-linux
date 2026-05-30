use axum::{extract::{Path, State}, http::StatusCode, routing::{get, post}, Json, Router};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::{info, error};

use crate::daemon::proto::{ErrorCode, Response, VpnStatus};
use crate::daemon::state::SharedState;
use crate::daemon::swanctl;
use crate::daemon::server::vpn_pin;
use crate::daemon::extra_handlers::{handle_reconnect, handle_pause};

use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const API_ADDR: &str = "127.10.0.18";
pub const API_PORT: u16 = 1600;
pub const HEALTH_PORT: u16 = 1601;

pub async fn run_http_api(state: SharedState) -> Result<(), String> {
    let app = Router::new()
        .route("/status", get(handle_status))
        .route("/servers", get(handle_servers))
        .route("/connect", post(handle_connect))
        .route("/disconnect", post(handle_disconnect))
        .route("/reconnect", post(handle_reconnect))
        .route("/pause", post(handle_pause))
        .route("/config", get(handle_get_config))
        .route("/config", post(handle_set_config))
        .route("/routing/rules", get(handle_list_rules))
        .route("/routing/rules", post(handle_add_rule))
        .route("/routing/rules/{id}", post(handle_update_rule))
        .route("/routing/rules/{id}/delete", post(handle_delete_rule))
        .route("/routing/rules/reorder", post(handle_reorder_rules))
        .route("/routing/apply", post(handle_apply_rules))
        .with_state(state.clone());

    let addr: SocketAddr = format!("{API_ADDR}:{API_PORT}")
        .parse()
        .map_err(|e| format!("bad addr: {e}"))?;

    let listener = TcpListener::bind(addr).await
        .map_err(|e| format!("bind {addr}: {e}"))?;
    info!("[http-api] listening on http://{addr}");

    tokio::spawn(run_health_server(state));

    axum::serve(listener, app).await
        .map_err(|e| format!("http-api serve: {e}"))
}

async fn run_health_server(state: SharedState) {
    let app = Router::new()
        .route("/health", get(handle_health))
        .with_state(state);

    let addr: SocketAddr = match format!("{API_ADDR}:{HEALTH_PORT}").parse() {
        Ok(a) => a,
        Err(e) => { error!("[health] bad addr: {e}"); return; }
    };

    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => { error!("[health] bind {addr}: {e}"); return; }
    };
    info!("[health] listening on http://{addr}/health");

    if let Err(e) = axum::serve(listener, app).await {
        error!("[health] serve: {e}");
    }
}

async fn handle_status(State(state): State<SharedState>) -> Json<VpnStatus> {
    let mut status = swanctl::live_status().await;
    let snap = state.read().await;
    status.authorized = snap.authorization_fresh();
    status.authorized_at = snap.authorized_at.map(format_iso_utc);
    status.country = snap.country.clone();
    Json(status)
}

async fn handle_servers() -> Json<Response> {
    // Fetch the FULL server list from the Privado Portal API (cached 5 min).
    let portal_servers = crate::daemon::portal_api::get_servers().await;
    let entries: Vec<crate::daemon::proto::ProvisionedServer> = portal_servers.iter()
        .map(|s| crate::daemon::proto::ProvisionedServer {
            country_code: s.country_code.to_lowercase(),
            display: if s.city.is_empty() {
                format!("{} ({})", s.country, s.country_code)
            } else {
                format!("{}, {} ({})", s.city, s.country, s.country_code)
            },
            remote_host: s.hostname.clone(),
        })
        .collect();
    Json(Response::Servers { entries })
}

/// Connect request body. Supports two modes:
/// 1. Simple: `{ "pin": "1234", "country": "nl" }` — uses Portal API server lookup
/// 2. Full:   `{ "pin": "1234", "server_host": "ams-101.vpn.privado.io", "username": "user",
///              "password": "pass", "routes": ["0.0.0.0/0"], "dns": ["198.18.0.1"],
///              "kill_switch": true }` — used by the Tauri UI for dynamic servers
#[derive(serde::Deserialize)]
struct ConnectReq {
    pin: String,
    /// Country code shortcut (nl/sg/mx). Used when server_host is not provided.
    #[serde(default)]
    country: String,
    /// Full server hostname for dynamic connections from the Tauri UI.
    #[serde(default)]
    server_host: Option<String>,
    /// EAP username (from Privado login). Falls back to config file if empty.
    #[serde(default)]
    username: Option<String>,
    /// EAP password. Falls back to config file if empty.
    #[serde(default)]
    password: Option<String>,
    /// Remote traffic selectors (CIDRs). Empty = full tunnel "0.0.0.0/0".
    #[serde(default)]
    routes: Vec<String>,
    /// DNS servers to push into the tunnel.
    #[serde(default)]
    dns: Vec<String>,
    /// Enable kill switch on connect.
    #[serde(default)]
    kill_switch: Option<bool>,
}

async fn handle_connect(
    State(state): State<SharedState>,
    Json(body): Json<ConnectReq>,
) -> (StatusCode, Json<Response>) {
    if body.pin != vpn_pin() {
        return (StatusCode::FORBIDDEN, Json(Response::Err {
            code: ErrorCode::BadPin,
            message: "PIN rejected".into(),
        }));
    }

    // Load config to fill in defaults.
    let cfg = crate::config::load_config().unwrap_or_default();

    // Determine the server hostname.
    let server_host = if let Some(ref host) = body.server_host {
        if host.is_empty() {
            return (StatusCode::BAD_REQUEST, Json(Response::Err {
                code: ErrorCode::BadRequest,
                message: "server_host is empty".into(),
            }));
        }
        host.clone()
    } else {
        // Look up the best server for this country from the Portal API server list.
        let cc = if body.country.is_empty() {
            cfg.preferred_country.as_deref().unwrap_or("nl").to_string()
        } else {
            body.country.clone()
        };
        match crate::daemon::portal_api::find_server_for_country(&cc).await {
            Some(server) => server.hostname,
            None => {
                // Fallback to the static hostname map if API unavailable.
                country_to_default_host(&cc)
            }
        }
    };

    // Determine credentials.
    let username = body.username.filter(|s| !s.is_empty())
        .unwrap_or_else(|| cfg.username.clone());
    let password = body.password.filter(|s| !s.is_empty())
        .unwrap_or_else(|| cfg.password.clone());

    if username.is_empty() || password.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(Response::Err {
            code: ErrorCode::BadRequest,
            message: "No credentials. Log in first (privado-vpn login or use the UI).".into(),
        }));
    }

    // Determine routes and DNS.
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

    // Ensure strongSwan is running.
    if let Err(e) = swanctl::ensure_strongswan_up().await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(Response::Err {
            code: ErrorCode::StrongswanError, message: e,
        }));
    }

    // Write swanctl config for this specific server.
    if let Err(e) = swanctl::write_dynamic_config(&server_host, &username, &password, &routes, &dns).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(Response::Err {
            code: ErrorCode::StrongswanError, message: e,
        }));
    }

    // Derive country from hostname for state tracking.
    let country_code = derive_country_from_host(&server_host);
    state.write().await.authorize(country_code.clone());

    // Initiate the connection with auto-protocol fallback.
    // If the preferred protocol (IKEv2) fails within 10 seconds, try the
    // next protocol in the user's preference list (WireGuard, then OpenVPN).
    let ikev2_result = swanctl::initiate_dynamic().await;
    if let Err(ref e) = ikev2_result {
        info!("[connect] IKEv2 failed ({e}), checking fallback protocols...");

        // Try WireGuard if available and in preference list.
        let wg_available = crate::daemon::wireguard::is_available().await;
        if wg_available && cfg.protocol_preference.contains(&"wireguard".to_string()) {
            info!("[connect] attempting WireGuard fallback");

            // Read the API token from the user's token.json so the daemon can
            // call Privado's WireGuard login endpoint directly.
            let token_path = crate::config::config_dir().join("token.json");
            let wg_token = std::fs::read_to_string(&token_path).ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .and_then(|v| v["access_token"].as_str().map(String::from));

            if let Some(token) = wg_token {
                // Use the first API server that responds for the WG login call.
                let api_bases = [
                    "https://f3556fm3o524m9.com",
                    "https://3nkh5crxol.ch:15748",
                    "https://qya97ge69i2loo.com:7491",
                ];
                let mut wg_connected = false;

                for api_base in &api_bases {
                    match crate::daemon::wireguard::wg_login(
                        api_base, &token, &username, &password, &server_host,
                    ).await {
                        Ok(wg_cfg) => {
                            match crate::daemon::wireguard::connect(&wg_cfg).await {
                                Ok(()) => {
                                    info!("[connect] WireGuard fallback succeeded via {api_base}");
                                    let mut st = state.write().await;
                                    st.active_protocol = crate::daemon::state::ActiveProtocol::WireGuard;
                                    st.current_server = Some(server_host.clone());
                                    drop(st);
                                    wg_connected = true;
                                    break;
                                }
                                Err(e) => {
                                    info!("[connect] WireGuard connect failed: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            info!("[connect] WireGuard login to {api_base} failed: {e}");
                            continue;
                        }
                    }
                }

                if wg_connected {
                    let dns_for_routing = dns.clone();
                    let split_domains = cfg.split_domains.clone();
                    let cfg_for_rules = cfg.clone();
                    let remote_ips = crate::routing::resolve_domain_ips(std::slice::from_ref(&server_host));
                    state.write().await.current_remote_ips = remote_ips.clone();
                    tokio::task::spawn_blocking(move || {
                        crate::routing::on_connect(&remote_ips, &dns_for_routing, kill_switch, &split_domains);
                        crate::daemon::routing_rules::apply_routing_rules(&cfg_for_rules);
                    });

                    tokio::time::sleep(Duration::from_millis(700)).await;
                    let mut status = swanctl::live_status().await;
                    status.connected = true;
                    let snap = state.read().await;
                    status.authorized = snap.authorization_fresh();
                    status.authorized_at = snap.authorized_at.map(format_iso_utc);
                    status.country = snap.country.clone();
                    return (StatusCode::OK, Json(Response::Ok { status }));
                }
            } else {
                info!("[connect] no API token on disk, skipping WireGuard fallback");
            }
        }

        // Try OpenVPN if available and in preference list.
        let ovpn_available = crate::daemon::openvpn::is_available().await;
        if ovpn_available && cfg.protocol_preference.contains(&"openvpn".to_string()) {
            info!("[connect] attempting OpenVPN fallback");
            let ovpn_cfg = crate::daemon::openvpn::OvpnConfig {
                server_host: server_host.clone(),
                port: 443,
                protocol: crate::daemon::openvpn::OvpnProtocol::Udp,
                username: username.clone(),
                password: password.clone(),
                ca_cert_path: String::new(),
                scramble: false,
                dns: dns.clone(),
            };
            match crate::daemon::openvpn::connect(&ovpn_cfg).await {
                Ok(()) => {
                    info!("[connect] OpenVPN fallback succeeded");
                    // Skip the IKEv2 error — OpenVPN is now active.
                    let mut st = state.write().await;
                    st.active_protocol = crate::daemon::state::ActiveProtocol::OpenVPN;
                    st.current_server = Some(server_host.clone());
                    drop(st);

                    let dns_for_routing = dns.clone();
                    let split_domains = cfg.split_domains.clone();
                    let cfg_for_rules = cfg.clone();
                    let remote_ips = crate::routing::resolve_domain_ips(std::slice::from_ref(&server_host));
                    state.write().await.current_remote_ips = remote_ips.clone();
                    tokio::task::spawn_blocking(move || {
                        crate::routing::on_connect(&remote_ips, &dns_for_routing, kill_switch, &split_domains);
                        crate::daemon::routing_rules::apply_routing_rules(&cfg_for_rules);
                    });

                    tokio::time::sleep(Duration::from_millis(700)).await;
                    let mut status = swanctl::live_status().await;
                    status.connected = true;
                    let snap = state.read().await;
                    status.authorized = snap.authorization_fresh();
                    status.authorized_at = snap.authorized_at.map(format_iso_utc);
                    status.country = snap.country.clone();
                    return (StatusCode::OK, Json(Response::Ok { status }));
                }
                Err(ovpn_err) => {
                    info!("[connect] OpenVPN fallback also failed: {ovpn_err}");
                }
            }
        }

        // All protocols failed — return the original IKEv2 error.
        state.write().await.revoke();
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(Response::Err {
            code: ErrorCode::StrongswanError, message: ikev2_result.unwrap_err(),
        }));
    }

    // IKEv2 succeeded.
    {
        let mut st = state.write().await;
        st.active_protocol = crate::daemon::state::ActiveProtocol::IKEv2;
        st.current_server = Some(server_host.clone());
    }

    // Resolve the remote endpoint IPs up front and persist them in daemon state
    // so disconnect can tear down the exact mangle MARK rules these created
    // (R5). Then install routing (policy routes + DNS override + rule engine +
    // kill switch) in the background.
    let remote_ips = crate::routing::resolve_domain_ips(std::slice::from_ref(&server_host));
    state.write().await.current_remote_ips = remote_ips.clone();
    let dns_for_routing = dns.clone();
    let split_domains = cfg.split_domains.clone();
    let cfg_for_rules = cfg.clone();
    tokio::task::spawn_blocking(move || {
        crate::routing::on_connect(
            &remote_ips,
            &dns_for_routing,
            kill_switch,
            &split_domains,
        );
        crate::daemon::routing_rules::apply_routing_rules(&cfg_for_rules);
    });

    tokio::time::sleep(Duration::from_millis(700)).await;

    let mut status = swanctl::live_status().await;
    let snap = state.read().await;
    status.authorized = snap.authorization_fresh();
    status.authorized_at = snap.authorized_at.map(format_iso_utc);
    status.country = snap.country.clone();
    (StatusCode::OK, Json(Response::Ok { status }))
}

#[derive(serde::Deserialize)]
struct DisconnectReq {
    pin: String,
}

async fn handle_disconnect(
    State(state): State<SharedState>,
    Json(body): Json<DisconnectReq>,
) -> (StatusCode, Json<Response>) {
    if body.pin != vpn_pin() {
        return (StatusCode::FORBIDDEN, Json(Response::Err {
            code: ErrorCode::BadPin,
            message: "PIN rejected".into(),
        }));
    }
    // Capture the resolved endpoint IPs BEFORE revoke() clears them, so the
    // per-IP mangle MARK rules installed at connect time are actually removed
    // (R5) instead of leaking after disconnect.
    let remote_ips = {
        let mut st = state.write().await;
        let ips = st.current_remote_ips.clone();
        st.revoke();
        ips
    };

    // Remove routing (including the rule engine's nft table) before terminating SAs.
    let _ = tokio::task::spawn_blocking(move || {
        crate::routing::on_disconnect(&remote_ips);
        crate::daemon::routing_rules::clear_routing_rules();
    }).await;

    swanctl::terminate_all_privado().await;
    swanctl::cleanup_config().await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    let status = swanctl::live_status().await;
    (StatusCode::OK, Json(Response::Ok { status }))
}

async fn handle_get_config() -> Json<serde_json::Value> {
    let cfg = crate::config::load_config().unwrap_or_default();
    Json(serde_json::json!({
        "username": cfg.username,
        "preferred_country": cfg.preferred_country,
        "preferred_city": cfg.preferred_city,
        "split_tunnel": cfg.split_tunnel,
        "split_domains": cfg.split_domains,
        "kill_switch": cfg.kill_switch,
        "auto_connect": cfg.auto_connect,
        "dns_servers": cfg.dns_servers,
        "trusted_networks": cfg.trusted_networks,
        "protocol": cfg.protocol,
        "route_llm_browser": cfg.route_llm_browser,
        "route_llm_tools": cfg.route_llm_tools,
        "routing_rules": cfg.routing_rules,
    }))
}

#[derive(serde::Deserialize)]
struct SetConfigReq {
    #[serde(default)] preferred_country: Option<String>,
    #[serde(default)] preferred_city: Option<String>,
    #[serde(default)] kill_switch: Option<bool>,
    #[serde(default)] auto_connect: Option<bool>,
    #[serde(default)] split_tunnel: Option<bool>,
    #[serde(default)] split_domains: Option<Vec<String>>,
    #[serde(default)] dns_servers: Option<Vec<String>>,
    #[serde(default)] trusted_networks: Option<Vec<String>>,
    #[serde(default)] protocol: Option<String>,
    #[serde(default)] route_llm_browser: Option<bool>,
    #[serde(default)] route_llm_tools: Option<bool>,
    #[serde(default)] routing_rules: Option<Vec<crate::config::RoutingRule>>,
}

async fn handle_set_config(Json(body): Json<SetConfigReq>) -> (StatusCode, Json<serde_json::Value>) {
    let mut cfg = crate::config::load_config().unwrap_or_default();
    // Capture a kill-switch toggle (Copy) before applying so we can act on it
    // after the save regardless of order.
    let kill_switch_change = body.kill_switch;
    // Track whether any routing-relevant field changed so we can live-re-apply
    // the rule engine, mirroring the kill_switch live-apply block.
    let routing_changed = body.routing_rules.is_some()
        || body.route_llm_browser.is_some()
        || body.route_llm_tools.is_some();
    if let Some(v) = body.preferred_country { cfg.preferred_country = Some(v); }
    if let Some(v) = body.preferred_city { cfg.preferred_city = Some(v); }
    if let Some(v) = body.kill_switch { cfg.kill_switch = v; }
    if let Some(v) = body.auto_connect { cfg.auto_connect = v; }
    if let Some(v) = body.split_tunnel { cfg.split_tunnel = v; }
    if let Some(v) = body.split_domains { cfg.split_domains = v; }
    if let Some(v) = body.dns_servers { cfg.dns_servers = v; }
    if let Some(v) = body.trusted_networks { cfg.trusted_networks = v; }
    if let Some(v) = body.protocol { cfg.protocol = v; }
    if let Some(v) = body.route_llm_browser { cfg.route_llm_browser = v; }
    if let Some(v) = body.route_llm_tools { cfg.route_llm_tools = v; }
    if let Some(v) = body.routing_rules { cfg.routing_rules = v; }
    match crate::config::save_config(&cfg) {
        Ok(_) => {
            // #2: apply a kill-switch toggle to routing LIVE when the VPN is
            // currently connected, so flipping it in Settings (GUI or CLI/LLM)
            // takes effect immediately instead of only on the next connect.
            if let Some(ks) = kill_switch_change {
                if crate::daemon::swanctl::live_status().await.connected {
                    let domains = cfg.split_domains.clone();
                    tokio::task::spawn_blocking(move || {
                        if ks {
                            let _ = crate::routing::install_killswitch(&domains);
                        } else {
                            let _ = crate::routing::remove_killswitch();
                        }
                    });
                }
            }
            // Mirror the kill_switch live-apply for the routing-rule engine:
            // re-derive and re-apply the nft table when connected so rule edits
            // (or route_llm_* toggles) take effect immediately.
            if routing_changed && crate::daemon::swanctl::live_status().await.connected {
                let cfg_for_rules = cfg.clone();
                tokio::task::spawn_blocking(move || {
                    crate::daemon::routing_rules::apply_routing_rules(&cfg_for_rules);
                });
            }
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))),
    }
}

// ─── Routing-rule CRUD endpoints ────────────────────────────────────────────
//
// All mutations are pin-gated like /connect. Each persists the config then
// live-re-applies the rule engine if the VPN is currently connected, so a rule
// change from the GUI/CLI/LLM takes effect immediately.

/// Persist `cfg.routing_rules` and re-apply the engine live when connected.
async fn persist_and_reapply(cfg: &crate::config::Config) -> Result<(), String> {
    crate::config::save_config(cfg)?;
    if crate::daemon::swanctl::live_status().await.connected {
        let cfg_for_rules = cfg.clone();
        tokio::task::spawn_blocking(move || {
            crate::daemon::routing_rules::apply_routing_rules(&cfg_for_rules);
        });
    }
    Ok(())
}

fn rules_ok(cfg: &crate::config::Config) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true, "rules": cfg.routing_rules }))
}

fn save_err(e: String) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e })))
}

fn bad_pin() -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": "PIN rejected" })))
}

async fn handle_list_rules() -> Json<serde_json::Value> {
    // Run the one-shot legacy migration lazily so the first read reflects any
    // pre-existing split_domains config even if the daemon predates migration.
    let mut cfg = crate::config::load_config().unwrap_or_default();
    if crate::config::migrate_split_domains_to_rules(&mut cfg) {
        let _ = crate::config::save_config(&cfg);
    }
    Json(serde_json::json!({ "rules": cfg.routing_rules }))
}

#[derive(serde::Deserialize)]
struct AddRuleReq {
    pin: String,
    rule: crate::config::RoutingRule,
}

async fn handle_add_rule(Json(body): Json<AddRuleReq>) -> (StatusCode, Json<serde_json::Value>) {
    if body.pin != vpn_pin() { return bad_pin(); }
    let mut cfg = crate::config::load_config().unwrap_or_default();
    let mut rule = body.rule;
    if rule.id.trim().is_empty() {
        rule.id = crate::config::gen_id();
    }
    // Append at the end; priority follows array order (highest existing + 1).
    let next_priority = cfg.routing_rules.iter().map(|r| r.priority).max().map(|m| m + 1).unwrap_or(0);
    if rule.priority == 0 {
        rule.priority = next_priority;
    }
    cfg.routing_rules.push(rule);
    match persist_and_reapply(&cfg).await {
        Ok(_) => (StatusCode::OK, rules_ok(&cfg)),
        Err(e) => save_err(e),
    }
}

#[derive(serde::Deserialize)]
struct UpdateRuleReq {
    pin: String,
    rule: crate::config::RoutingRule,
}

async fn handle_update_rule(
    Path(id): Path<String>,
    Json(body): Json<UpdateRuleReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    if body.pin != vpn_pin() { return bad_pin(); }
    let mut cfg = crate::config::load_config().unwrap_or_default();
    match cfg.routing_rules.iter_mut().find(|r| r.id == id) {
        Some(slot) => {
            let mut updated = body.rule;
            // Keep the path id authoritative.
            updated.id = id.clone();
            *slot = updated;
        }
        None => {
            return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "rule not found" })));
        }
    }
    match persist_and_reapply(&cfg).await {
        Ok(_) => (StatusCode::OK, rules_ok(&cfg)),
        Err(e) => save_err(e),
    }
}

#[derive(serde::Deserialize)]
struct PinOnlyReq {
    pin: String,
}

async fn handle_delete_rule(
    Path(id): Path<String>,
    Json(body): Json<PinOnlyReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    if body.pin != vpn_pin() { return bad_pin(); }
    let mut cfg = crate::config::load_config().unwrap_or_default();
    let before = cfg.routing_rules.len();
    cfg.routing_rules.retain(|r| r.id != id);
    if cfg.routing_rules.len() == before {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "rule not found" })));
    }
    match persist_and_reapply(&cfg).await {
        Ok(_) => (StatusCode::OK, rules_ok(&cfg)),
        Err(e) => save_err(e),
    }
}

#[derive(serde::Deserialize)]
struct ReorderReq {
    pin: String,
    order: Vec<String>,
}

async fn handle_reorder_rules(Json(body): Json<ReorderReq>) -> (StatusCode, Json<serde_json::Value>) {
    if body.pin != vpn_pin() { return bad_pin(); }
    let mut cfg = crate::config::load_config().unwrap_or_default();
    // Reorder rules by the provided id sequence, assigning priorities in steps
    // of 10. Any rule not named in `order` keeps its relative position after.
    let mut reordered: Vec<crate::config::RoutingRule> = Vec::with_capacity(cfg.routing_rules.len());
    for id in &body.order {
        if let Some(pos) = cfg.routing_rules.iter().position(|r| &r.id == id) {
            reordered.push(cfg.routing_rules.remove(pos));
        }
    }
    reordered.append(&mut cfg.routing_rules);
    for (i, r) in reordered.iter_mut().enumerate() {
        r.priority = (i as u32) * 10;
    }
    cfg.routing_rules = reordered;
    match persist_and_reapply(&cfg).await {
        Ok(_) => (StatusCode::OK, rules_ok(&cfg)),
        Err(e) => save_err(e),
    }
}

async fn handle_apply_rules(Json(body): Json<PinOnlyReq>) -> (StatusCode, Json<serde_json::Value>) {
    if body.pin != vpn_pin() { return bad_pin(); }
    let cfg = crate::config::load_config().unwrap_or_default();
    let connected = crate::daemon::swanctl::live_status().await.connected;
    if connected {
        let cfg_for_rules = cfg.clone();
        tokio::task::spawn_blocking(move || {
            crate::daemon::routing_rules::apply_routing_rules(&cfg_for_rules);
        });
    }
    (StatusCode::OK, Json(serde_json::json!({ "ok": true, "applied": connected, "rules": cfg.routing_rules })))
}

#[derive(serde::Serialize)]
struct HealthResponse {
    status: String,
    daemon_pid: u32,
    authorized: bool,
    uptime_secs: u64,
    established_sas: usize,
}

async fn handle_health(State(state): State<SharedState>) -> Json<HealthResponse> {
    let snap = state.read().await;
    let sa_count = swanctl::count_established_privado_sas().await;
    Json(HealthResponse {
        status: "ok".into(),
        daemon_pid: std::process::id(),
        authorized: snap.authorization_fresh(),
        uptime_secs: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        established_sas: sa_count,
    })
}

/// Map a country code to a default server hostname for auto-connect.
pub fn country_to_default_host(country: &str) -> String {
    match country.to_lowercase().as_str() {
        "nl" => "ams-101.vpn.privado.io".into(),
        "sg" => "sin-005.vpn.privado.io".into(),
        "mx" => "mex-011.vpn.privado.io".into(),
        "gb" | "uk" => "lon-101.vpn.privado.io".into(),
        "de" => "fra-101.vpn.privado.io".into(),
        "us" => "nyc-101.vpn.privado.io".into(),
        "ca" => "tor-101.vpn.privado.io".into(),
        "jp" => "tok-101.vpn.privado.io".into(),
        "au" => "syd-101.vpn.privado.io".into(),
        "fr" => "par-101.vpn.privado.io".into(),
        "ch" => "zrh-101.vpn.privado.io".into(),
        _ => "ams-101.vpn.privado.io".into(),
    }
}

/// Derive a 2-letter country code from a hostname like "ams-101.vpn.privado.io"
pub fn derive_country_from_host(host: &str) -> String {
    let prefix = host.split('.').next().unwrap_or("");
    match prefix {
        s if s.starts_with("ams") => "nl".into(),
        s if s.starts_with("sin") => "sg".into(),
        s if s.starts_with("mex") => "mx".into(),
        s if s.starts_with("lon") => "gb".into(),
        s if s.starts_with("fra") => "de".into(),
        s if s.starts_with("nyc") || s.starts_with("lax") || s.starts_with("mia") || s.starts_with("chi") || s.starts_with("dal") => "us".into(),
        s if s.starts_with("tor") || s.starts_with("van") || s.starts_with("mon") => "ca".into(),
        s if s.starts_with("tok") => "jp".into(),
        s if s.starts_with("syd") || s.starts_with("mel") => "au".into(),
        s if s.starts_with("par") => "fr".into(),
        s if s.starts_with("zrh") || s.starts_with("zur") => "ch".into(),
        _ => "xx".into(),
    }
}

fn format_iso_utc(t: SystemTime) -> String {
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
