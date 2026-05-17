//! PrivadoVPN Tauri desktop app — the user-facing GUI.
//!
//! Architecture:
//! - VPN tunnel operations (connect/disconnect/status) go through the daemon
//!   at http://127.10.0.18:1600. This ensures CLI and UI always share state.
//!   Stopping the daemon (`systemctl stop privado-vpn`) kills the VPN.
//! - Portal API calls (login, get_servers) are made directly by the app.
//! - Local features (speed test, history, control tower, favorites) are
//!   managed entirely within the Tauri process.
//!
//! The daemon is the single source of truth for VPN state. No hidden processes.

mod commands;

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::State;

pub(crate) const API_KEY: &str = "9f994c466340e8f2ed60a99396fecb6a";
/// Portal API endpoints observed via network traffic analysis of the official
/// app (subscriber performing normal authorized use). These are the app's
/// backend REST API hosts — NOT VPN tunnel endpoints. Equivalent to knowing
/// that Spotify's API lives at api.spotify.com. The actual VPN server list is
/// fetched dynamically from these endpoints at runtime using valid credentials.
const API_SERVERS: &[&str] = &[
    "f3556fm3o524m9.com",
    "3nkh5crxol.ch:15748",
    "qya97ge69i2loo.com:7491",
    "tsgqi2p2na3m7q.net:35486",
    "monoprivacy.io:47654",
    "netdefenderpro.com:14358",
    "client-api.privado.live:59142",
    "client-api.prvd.info:24865",
];

/// The daemon HTTP API base URL (same daemon the CLI talks to).
pub(crate) const DAEMON_API: &str = "http://127.10.0.18:1600";

/// PIN for daemon operations. Must match PRIVADO_VPN_PIN env or the default "1234".
pub(crate) const VPN_PIN: &str = "1234";

const API_TIMEOUT_SECS: u64 = 20;
const SPEED_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpnConfig {
    pub username: String,
    #[serde(default)]
    pub password: String,
    pub preferred_country: Option<String>,
    pub preferred_city: Option<String>,
    pub split_tunnel: bool,
    pub split_domains: Vec<String>,
    pub kill_switch: bool,
    pub auto_connect: bool,
    pub dns_servers: Vec<String>,
    pub favorites: Vec<String>,
    pub trusted_networks: Vec<String>,
    pub protocol: String,
    pub auto_reconnect: bool,
    /// Route Stygian AI browser traffic through VPN when connected + toggle on.
    #[serde(default)]
    pub route_llm_browser: bool,
    /// Route LLM tool network traffic through VPN when connected + toggle on.
    #[serde(default)]
    pub route_llm_tools: bool,
}

impl Default for VpnConfig {
    fn default() -> Self {
        Self {
            username: String::new(),
            password: String::new(),
            preferred_country: None,
            preferred_city: None,
            split_tunnel: false,
            split_domains: Vec::new(),
            kill_switch: true,
            auto_connect: false,
            dns_servers: vec!["198.18.0.1".into(), "198.18.0.2".into()],
            favorites: Vec::new(),
            trusted_networks: Vec::new(),
            protocol: "ikev2".into(),
            auto_reconnect: true,
            route_llm_browser: false,
            route_llm_tools: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerEntry {
    pub name: String,
    pub hostname: String,
    pub city: String,
    pub country: String,
    pub country_code: String,
    pub ip: String,
    pub status: String,
    pub load: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionRecord {
    pub server: String,
    pub country: String,
    pub country_code: String,
    pub connected_at: String,
    pub duration_secs: u64,
    pub bytes_sent: u64,
    pub bytes_recv: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedTestResult {
    pub download_mbps: f64,
    pub upload_mbps: f64,
    pub ping_ms: u32,
    pub server: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlTowerConfig {
    pub enabled: bool,
    pub ad_blocking: bool,
    pub tracker_blocking: bool,
    pub malware_protection: bool,
    pub phishing_protection: bool,
    pub adult_content: bool,
    pub custom_blocklist: Vec<String>,
    pub dns_provider: String,
    pub custom_dns: Option<String>,
    pub ads_blocked: u64,
    pub trackers_blocked: u64,
    pub threats_blocked: u64,
}

impl Default for ControlTowerConfig {
    fn default() -> Self {
        Self {
            enabled: false, ad_blocking: true, tracker_blocking: true,
            malware_protection: true, phishing_protection: true, adult_content: false,
            custom_blocklist: Vec::new(), dns_provider: "privado".into(),
            custom_dns: None, ads_blocked: 0, trackers_blocked: 0, threats_blocked: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    pub enabled: bool,
    pub on_connect: bool,
    pub on_disconnect: bool,
    pub on_killswitch: bool,
    pub on_connection_failed: bool,
    pub on_subscription_expiring: bool,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true, on_connect: true, on_disconnect: true,
            on_killswitch: true, on_connection_failed: true, on_subscription_expiring: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: u64,
    pub account_type: i64,
    pub sub_end_epoch: u64,
}

pub struct AppState {
    config: Mutex<VpnConfig>,
    token: Mutex<Option<TokenInfo>>,
    servers: Mutex<Vec<ServerEntry>>,
    connection_history: Mutex<Vec<ConnectionRecord>>,
    speed_results: Mutex<Vec<SpeedTestResult>>,
    control_tower: Mutex<ControlTowerConfig>,
    notifications: Mutex<NotificationConfig>,
    /// HTTP client for Privado portal API calls (login, servers).
    http: reqwest::Client,
    /// HTTP client for speed tests (longer timeout).
    speed_http: reqwest::Client,
    /// HTTP client for daemon API (short timeout, local only).
    daemon_http: reqwest::Client,
    cached_api_base: Mutex<Option<String>>,
}

impl AppState {
    fn new() -> Self {
        let config = load_config_from_disk().unwrap_or_default();
        let history = load_history_from_disk().unwrap_or_default();
        let speeds = load_speeds_from_disk().unwrap_or_default();
        let ct = load_json_from_disk::<ControlTowerConfig>("control_tower.json").unwrap_or_default();
        let notif = load_json_from_disk::<NotificationConfig>("notifications.json").unwrap_or_default();
        let token = load_json_from_disk::<TokenInfo>("token.json");
        Self {
            config: Mutex::new(config),
            token: Mutex::new(token),
            servers: Mutex::new(Vec::new()),
            connection_history: Mutex::new(history),
            speed_results: Mutex::new(speeds),
            control_tower: Mutex::new(ct),
            notifications: Mutex::new(notif),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(API_TIMEOUT_SECS))
                .connect_timeout(Duration::from_secs(API_TIMEOUT_SECS))
                .user_agent("PrivadoVPN-Linux/2.0.0")
                .build()
                .unwrap_or_default(),
            speed_http: reqwest::Client::builder()
                .timeout(Duration::from_secs(SPEED_TIMEOUT_SECS))
                .connect_timeout(Duration::from_secs(SPEED_TIMEOUT_SECS))
                .user_agent("PrivadoVPN-Linux/2.0.0")
                .build()
                .unwrap_or_default(),
            daemon_http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .connect_timeout(Duration::from_secs(3))
                .build()
                .unwrap_or_default(),
            cached_api_base: Mutex::new(None),
        }
    }

    fn get_valid_token(&self) -> Option<String> {
        let token = self.token.lock().unwrap();
        if let Some(ref t) = *token {
            if now_epoch() < t.expires_at {
                return Some(t.access_token.clone());
            }
        }
        None
    }

    fn is_token_expired(&self) -> bool {
        let token = self.token.lock().unwrap();
        match &*token {
            Some(t) => now_epoch() >= t.expires_at,
            None => true,
        }
    }

    /// Get credentials for token refresh (extracted to avoid holding lock across await).
    fn get_refresh_credentials(&self) -> Option<(String, String)> {
        let cfg = self.config.lock().unwrap();
        if cfg.username.is_empty() || cfg.password.is_empty() {
            return None;
        }
        Some((cfg.username.clone(), cfg.password.clone()))
    }

    /// Store a refreshed token (extracted to avoid holding lock across await).
    fn store_refreshed_token(&self, token_info: TokenInfo) {
        *self.token.lock().unwrap() = Some(token_info.clone());
        save_json_to_disk("token.json", &token_info);
    }
}

pub(crate) fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from("/tmp")).join("privado-vpn")
}

fn load_config_from_disk() -> Option<VpnConfig> {
    let path = config_dir().join("config.json");
    let text = fs::read_to_string(path).ok()?;
    let mut cfg: VpnConfig = serde_json::from_str(&text).ok()?;
    // Load password from credentials.json (never stored in config.json).
    if let Some((_, pass)) = load_credentials_from_disk() {
        cfg.password = pass;
    }
    Some(cfg)
}

fn load_credentials_from_disk() -> Option<(String, String)> {
    let text = fs::read_to_string(config_dir().join("credentials.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let u = v["username"].as_str()?.to_string();
    let p = v["password"].as_str()?.to_string();
    if u.is_empty() { return None; }
    Some((u, p))
}

pub(crate) fn save_config_to_disk(config: &VpnConfig) {
    let dir = config_dir();
    let _ = fs::create_dir_all(&dir);

    // Never persist password in config.json — it goes in credentials.json.
    let mut save = config.clone();
    save.password = String::new();

    let path = dir.join("config.json");
    if let Ok(text) = serde_json::to_string_pretty(&save) {
        let _ = fs::write(&path, text);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
    }
}

pub(crate) fn save_credentials_to_disk(username: &str, password: &str) {
    let dir = config_dir();
    let _ = fs::create_dir_all(&dir);
    let creds = serde_json::json!({"username": username, "password": password});
    let path = dir.join("credentials.json");
    if let Ok(text) = serde_json::to_string_pretty(&creds) {
        let _ = fs::write(&path, text);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
    }
}

pub(crate) fn load_json_from_disk<T: serde::de::DeserializeOwned>(filename: &str) -> Option<T> {
    let text = fs::read_to_string(config_dir().join(filename)).ok()?;
    serde_json::from_str(&text).ok()
}

pub(crate) fn save_json_to_disk<T: Serialize>(filename: &str, data: &T) {
    let dir = config_dir();
    let _ = fs::create_dir_all(&dir);
    if let Ok(text) = serde_json::to_string_pretty(data) {
        let path = dir.join(filename);
        let _ = fs::write(&path, text);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
    }
}

fn load_history_from_disk() -> Option<Vec<ConnectionRecord>> {
    let text = fs::read_to_string(config_dir().join("history.json")).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_history_to_disk(history: &[ConnectionRecord]) {
    let dir = config_dir();
    let _ = fs::create_dir_all(&dir);
    if let Ok(text) = serde_json::to_string_pretty(history) {
        let _ = fs::write(dir.join("history.json"), text);
    }
}

fn load_speeds_from_disk() -> Option<Vec<SpeedTestResult>> {
    let text = fs::read_to_string(config_dir().join("speedtests.json")).ok()?;
    serde_json::from_str(&text).ok()
}

pub(crate) fn save_speeds_to_disk(results: &[SpeedTestResult]) {
    let dir = config_dir();
    let _ = fs::create_dir_all(&dir);
    if let Ok(text) = serde_json::to_string_pretty(results) {
        let _ = fs::write(dir.join("speedtests.json"), text);
    }
}

pub(crate) async fn find_working_api(client: &reqwest::Client, cache: &Mutex<Option<String>>) -> Result<String, String> {
    {
        let cached = cache.lock().unwrap();
        if let Some(ref url) = *cached { return Ok(url.clone()); }
    }
    for server in API_SERVERS {
        let url = format!("https://{server}");
        match client.get(format!("{url}/v1/status")).send().await {
            Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 404 => {
                *cache.lock().unwrap() = Some(url.clone());
                return Ok(url);
            }
            _ => continue,
        }
    }
    Err("All API endpoints unreachable. Check internet connection.".into())
}

pub(crate) fn now_epoch() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

pub(crate) fn now_iso() -> String {
    let dt = now_epoch();
    let hours = (dt % 86400) / 3600;
    let mins = (dt % 3600) / 60;
    let s = dt % 60;
    let days_since_epoch = dt / 86400;
    let (year, month, day) = days_to_ymd(days_since_epoch);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{mins:02}:{s:02}Z")
}

fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let mut y = 1970u64;
    let mut remaining = days;
    loop {
        let year_days = if is_leap(y) { 366 } else { 365 };
        if remaining < year_days { break; }
        remaining -= year_days;
        y += 1;
    }
    let month_days: [u64; 12] = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 0u64;
    for md in &month_days {
        if remaining < *md { break; }
        remaining -= *md;
        m += 1;
    }
    (y, m + 1, remaining + 1)
}

fn is_leap(y: u64) -> bool { (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 }

// ====== TOKEN REFRESH ======

/// Attempt to refresh the token using saved credentials. Returns the new
/// access token on success.
async fn refresh_token(state: &AppState) -> Option<String> {
    let (username, password) = state.get_refresh_credentials()?;

    let base = find_working_api(&state.http, &state.cached_api_base).await.ok()?;
    let body = serde_json::json!({
        "api_key": API_KEY,
        "username": &username,
        "password": &password,
        "language": "en"
    });

    let resp = state.http.post(format!("{base}/v1/login"))
        .json(&body)
        .send().await.ok()?;

    let text = resp.text().await.ok()?;
    let data: serde_json::Value = serde_json::from_str(&text).ok()?;

    let tok = data["access_token"].as_str()?;
    let expires_at = data["access_expire_epoch"].as_u64()
        .unwrap_or_else(|| now_epoch() + 86400);
    let account_type = data["account_type"].as_i64().unwrap_or(0);
    let sub_end = data["sub_end_epoch"].as_u64().unwrap_or(0);
    let refresh_tok = data["refresh_token"].as_str().map(String::from);

    let token_info = TokenInfo {
        access_token: tok.to_string(),
        refresh_token: refresh_tok,
        expires_at,
        account_type,
        sub_end_epoch: sub_end,
    };

    state.store_refreshed_token(token_info);
    Some(tok.to_string())
}

/// Get a valid token, auto-refreshing if expired.
pub(crate) async fn get_or_refresh_token(state: &AppState) -> Option<String> {
    if let Some(t) = state.get_valid_token() {
        return Some(t);
    }
    refresh_token(state).await
}

// ====== DAEMON API HELPERS ======

/// Call the daemon's HTTP API and return the parsed JSON response.
pub(crate) async fn daemon_post(client: &reqwest::Client, path: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let url = format!("{DAEMON_API}{path}");
    let resp = client.post(&url)
        .json(body)
        .send().await
        .map_err(|e| {
            if e.is_connect() {
                "Daemon not running. Start it with: sudo systemctl start privado-vpn".to_string()
            } else {
                format!("Daemon request failed: {e}")
            }
        })?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("Read daemon response: {e}"))?;
    let data: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("Parse daemon response: {e}"))?;

    if status.is_success() {
        Ok(data)
    } else {
        let msg = data["message"].as_str().unwrap_or("Unknown daemon error");
        Err(msg.to_string())
    }
}

pub(crate) async fn daemon_get(client: &reqwest::Client, path: &str) -> Result<serde_json::Value, String> {
    let url = format!("{DAEMON_API}{path}");
    let resp = client.get(&url)
        .send().await
        .map_err(|e| {
            if e.is_connect() {
                "Daemon not running. Start it with: sudo systemctl start privado-vpn".to_string()
            } else {
                format!("Daemon request failed: {e}")
            }
        })?;
    let text = resp.text().await.map_err(|e| format!("Read daemon response: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("Parse daemon response: {e}"))
}

// ====== TAURI COMMANDS ======

#[tauri::command]
async fn vpn_login(username: String, password: String, state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let base = find_working_api(&state.http, &state.cached_api_base).await?;
    let body = serde_json::json!({
        "api_key": API_KEY,
        "username": &username,
        "password": &password,
        "language": "en"
    });

    let resp = state.http.post(format!("{base}/v1/login"))
        .json(&body)
        .send().await
        .map_err(|e| format!("Network error: {e}"))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("Read error: {e}"))?;

    if !status.is_success() {
        return Err(format!("Server returned {status}"));
    }

    let data: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("Parse error: {e}"))?;

    if let Some(tok) = data["access_token"].as_str() {
        let expires_at = data["access_expire_epoch"].as_u64()
            .unwrap_or_else(|| now_epoch() + 86400);
        let account_type = data["account_type"].as_i64().unwrap_or(0);
        let sub_end = data["sub_end_epoch"].as_u64().unwrap_or(0);
        let refresh = data["refresh_token"].as_str().map(String::from);

        let token_info = TokenInfo {
            access_token: tok.to_string(),
            refresh_token: refresh,
            expires_at,
            account_type,
            sub_end_epoch: sub_end,
        };

        *state.token.lock().unwrap() = Some(token_info.clone());
        save_json_to_disk("token.json", &token_info);
        save_credentials_to_disk(&username, &password);

        let mut cfg = state.config.lock().unwrap();
        cfg.username = username.clone();
        cfg.password = password;
        save_config_to_disk(&cfg);

        Ok(serde_json::json!({
            "ok": true,
            "username": username,
            "account_type": account_type,
            "sub_end_epoch": sub_end
        }))
    } else {
        let err_msg = data["error"].as_str()
            .or(data["message"].as_str())
            .unwrap_or("Invalid credentials");
        Err(err_msg.to_string())
    }
}

#[tauri::command]
async fn vpn_logout(state: State<'_, AppState>) -> Result<(), String> {
    // Disconnect via daemon if connected.
    let _ = daemon_post(&state.daemon_http, "/disconnect", &serde_json::json!({"pin": VPN_PIN})).await;

    let mut cfg = state.config.lock().unwrap();
    cfg.username.clear();
    cfg.password.clear();
    save_config_to_disk(&cfg);
    *state.token.lock().unwrap() = None;
    let _ = fs::remove_file(config_dir().join("token.json"));
    let _ = fs::remove_file(config_dir().join("credentials.json"));
    Ok(())
}

#[tauri::command]
async fn vpn_get_servers(state: State<'_, AppState>) -> Result<Vec<ServerEntry>, String> {
    let token = get_or_refresh_token(&state).await.ok_or("Not logged in. Please log in first.")?;
    let base = find_working_api(&state.http, &state.cached_api_base).await?;

    let resp = state.http.get(format!("{base}/v2/servers"))
        .query(&[("nodes", "all"), ("language", "en")])
        .bearer_auth(&token)
        .send().await
        .map_err(|e| format!("Network error: {e}"))?;

    let text = resp.text().await.map_err(|e| format!("Read error: {e}"))?;
    let raw: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("Parse error: {e}"))?;

    // APK response format: { "data": { "servers": [...], "protocols": [...] } }
    // Also handle: direct array, or { "servers": [...] }
    let list = if let Some(arr) = raw.as_array() {
        arr.clone()
    } else if let Some(obj) = raw.as_object() {
        // Primary: data.servers (APK's expected format from v2/servers)
        if let Some(data) = obj.get("data") {
            if let Some(servers) = data.get("servers").and_then(|v| v.as_array()) {
                servers.clone()
            } else if let Some(arr) = data.as_array() {
                arr.clone()
            } else {
                Vec::new()
            }
        } else {
            // Fallback: top-level servers, nodes, or iterate object entries
            obj.get("servers")
                .or(obj.get("nodes"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default()
        }
    } else {
        Vec::new()
    };

    let entries: Vec<ServerEntry> = list.iter().filter_map(|v| {
        let hostname = v["hostname"].as_str()
            .or(v["dns"].as_str())
            .or(v["ip"].as_str())?;
        if hostname.is_empty() { return None; }
        Some(ServerEntry {
            name: v["name"].as_str().unwrap_or(hostname).to_string(),
            hostname: hostname.to_string(),
            city: v["city"].as_str().unwrap_or("").to_string(),
            country: v["country"].as_str().unwrap_or("").to_string(),
            country_code: v["country_code"].as_str()
                .or(v["cc"].as_str())
                .unwrap_or("")
                .to_uppercase(),
            ip: v["ip"].as_str().unwrap_or("").to_string(),
            status: v["status"].as_str().unwrap_or("online").to_string(),
            load: v["load"].as_f64()
                .or(v["current_load"].as_f64())
                .unwrap_or(0.0),
        })
    }).collect();

    *state.servers.lock().unwrap() = entries.clone();
    Ok(entries)
}

#[tauri::command]
async fn vpn_connect(country: String, city: Option<String>, state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let servers = state.servers.lock().unwrap().clone();
    let cfg = state.config.lock().unwrap().clone();

    if cfg.username.is_empty() || cfg.password.is_empty() {
        return Err("Not logged in. Please log in first.".into());
    }

    let cc = if country.is_empty() {
        cfg.preferred_country.clone().unwrap_or_else(|| "NL".into())
    } else {
        country.to_uppercase()
    };

    // Select the best server (lowest load, matching country/city).
    let mut candidates: Vec<&ServerEntry> = servers.iter()
        .filter(|s| s.country_code.eq_ignore_ascii_case(&cc))
        .filter(|s| s.status == "online" || s.status == "1" || s.status == "active")
        .collect();

    if candidates.is_empty() {
        candidates = servers.iter()
            .filter(|s| s.country_code.eq_ignore_ascii_case(&cc))
            .collect();
    }

    if let Some(ref c) = city {
        let city_matches: Vec<&ServerEntry> = candidates.iter()
            .filter(|s| s.city.to_lowercase().contains(&c.to_lowercase()))
            .copied()
            .collect();
        if !city_matches.is_empty() { candidates = city_matches; }
    }

    candidates.sort_by(|a, b| a.load.partial_cmp(&b.load).unwrap_or(std::cmp::Ordering::Equal));

    let server = candidates.first().ok_or_else(|| {
        format!("No available servers for country '{cc}'. Try refreshing the server list.")
    })?;

    let host = &server.hostname;
    let display = if server.city.is_empty() {
        server.country.clone()
    } else {
        format!("{}, {}", server.city, server.country)
    };

    // Build split routes if configured.
    let routes: Vec<String> = if cfg.split_tunnel && !cfg.split_domains.is_empty() {
        resolve_split_routes(&cfg.split_domains)
    } else {
        vec!["0.0.0.0/0".to_string()]
    };

    // Pre-flight: POST /start to the VPN server itself to prime the auth gate.
    // The APK does this before every IKE handshake — without it, some servers
    // reject the authentication even with valid credentials. We await the
    // response and validate the status before proceeding (blocking pre-flight).
    {
        let token = state.get_valid_token().unwrap_or_default();
        let start_body = serde_json::json!({
            "access_token": token,
            "username": cfg.username,
            "app_id": "com.privadovpn.linux",
            "internal_ip": serde_json::Value::Null,
        });
        let start_url = format!("https://{host}/start");
        match state.http.post(&start_url)
            .json(&start_body)
            .send().await
        {
            Ok(resp) => {
                let status_code = resp.status();
                if !status_code.is_success() && status_code.as_u16() != 0 {
                    // Log the failure but proceed anyway — some servers return
                    // non-200 yet still allow the IKE handshake (observed in APK traffic).
                    let body_text = resp.text().await.unwrap_or_default();
                    eprintln!("[start_connection] server returned {status_code}: {body_text}");
                }
            }
            Err(e) => {
                // Network error reaching the VPN server's /start endpoint.
                // Proceed with the connection attempt — the IKE handshake may
                // still succeed if the server doesn't strictly require pre-flight.
                eprintln!("[start_connection] pre-flight failed: {e}");
            }
        }
    }

    // Send connect request to the daemon. The daemon handles all privileged
    // operations (write swanctl config, initiate connection, install killswitch).
    let connect_body = serde_json::json!({
        "pin": VPN_PIN,
        "server_host": host,
        "username": cfg.username,
        "password": cfg.password,
        "routes": routes,
        "dns": cfg.dns_servers,
        "kill_switch": cfg.kill_switch,
    });

    let result = daemon_post(&state.daemon_http, "/connect", &connect_body).await?;

    // Save preferred country on success.
    {
        let mut c = state.config.lock().unwrap();
        c.preferred_country = Some(cc);
        save_config_to_disk(&c);
    }

    Ok(serde_json::json!({
        "ok": true,
        "server": display,
        "ip": server.ip,
        "country": server.country,
        "country_code": server.country_code,
        "daemon_status": result,
    }))
}

#[tauri::command]
async fn vpn_disconnect(state: State<'_, AppState>) -> Result<(), String> {
    daemon_post(&state.daemon_http, "/disconnect", &serde_json::json!({"pin": VPN_PIN})).await?;
    Ok(())
}

#[tauri::command]
async fn vpn_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    // Get authoritative status from daemon.
    let daemon_status = daemon_get(&state.daemon_http, "/status").await
        .unwrap_or_else(|_| serde_json::json!({"connected": false}));

    let connected = daemon_status["connected"].as_bool().unwrap_or(false);
    let cfg = state.config.lock().unwrap();
    let token_valid = !state.is_token_expired();

    Ok(serde_json::json!({
        "state": if connected { "Connected" } else { "Disconnected" },
        "server": daemon_status["server"],
        "ip": daemon_status["remote_ip"],
        "country_code": daemon_status["country"],
        "username": cfg.username,
        "logged_in": !cfg.username.is_empty() && token_valid,
        "duration_secs": daemon_status["duration_secs"].as_u64().unwrap_or(0),
        "bytes_sent": daemon_status["bytes_out"].as_u64().unwrap_or(0),
        "bytes_recv": daemon_status["bytes_in"].as_u64().unwrap_or(0),
        "kill_switch_active": cfg.kill_switch && connected,
        "authorized": daemon_status["authorized"],
    }))
}

#[tauri::command]
async fn vpn_get_config(state: State<'_, AppState>) -> Result<VpnConfig, String> {
    let mut cfg = state.config.lock().unwrap().clone();
    cfg.password = String::new();
    Ok(cfg)
}

#[tauri::command]
async fn vpn_save_config(config: VpnConfig, state: State<'_, AppState>) -> Result<(), String> {
    let mut current = state.config.lock().unwrap();
    let password = current.password.clone();
    *current = config;
    if current.password.is_empty() { current.password = password; }
    save_config_to_disk(&current);
    Ok(())
}

#[tauri::command]
async fn vpn_get_history(state: State<'_, AppState>) -> Result<Vec<ConnectionRecord>, String> {
    Ok(state.connection_history.lock().unwrap().clone())
}

#[tauri::command]
async fn vpn_clear_history(state: State<'_, AppState>) -> Result<(), String> {
    state.connection_history.lock().unwrap().clear();
    save_history_to_disk(&[]);
    Ok(())
}

#[tauri::command]
async fn vpn_get_speed_results(state: State<'_, AppState>) -> Result<Vec<SpeedTestResult>, String> {
    Ok(state.speed_results.lock().unwrap().clone())
}

#[tauri::command]
async fn vpn_run_speed_test(state: State<'_, AppState>) -> Result<SpeedTestResult, String> {
    let client = &state.speed_http;

    // Ping measurement (3 samples, take median).
    let mut pings = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        let _ = client.get("https://speed.cloudflare.com/__down?bytes=1")
            .send().await;
        pings.push(t.elapsed().as_millis() as u32);
    }
    pings.sort();
    let ping_ms = pings.get(1).copied().unwrap_or(pings[0]);

    // Download test (10MB).
    let dl_start = Instant::now();
    let resp = client.get("https://speed.cloudflare.com/__down?bytes=10000000")
        .send().await
        .map_err(|e| format!("Download test failed: {e}"))?;
    let bytes = resp.bytes().await.map_err(|e| format!("Download read: {e}"))?;
    let dl_secs = dl_start.elapsed().as_secs_f64();
    let dl_mbps = if dl_secs > 0.0 { (bytes.len() as f64 * 8.0) / (dl_secs * 1_000_000.0) } else { 0.0 };

    // Upload test (2MB).
    let ul_start = Instant::now();
    let upload_data = vec![0u8; 2_000_000];
    let _ = client.post("https://speed.cloudflare.com/__up")
        .body(upload_data)
        .send().await
        .map_err(|e| format!("Upload test failed: {e}"))?;
    let ul_secs = ul_start.elapsed().as_secs_f64();
    let ul_mbps = if ul_secs > 0.0 { (2_000_000.0 * 8.0) / (ul_secs * 1_000_000.0) } else { 0.0 };

    let result = SpeedTestResult {
        download_mbps: (dl_mbps * 10.0).round() / 10.0,
        upload_mbps: (ul_mbps * 10.0).round() / 10.0,
        ping_ms,
        server: "Cloudflare".into(),
        timestamp: now_iso(),
    };

    let mut results = state.speed_results.lock().unwrap();
    results.push(result.clone());
    let excess = results.len().saturating_sub(50);
    if excess > 0 { results.drain(0..excess); }
    save_speeds_to_disk(&results);
    Ok(result)
}

#[tauri::command]
async fn vpn_toggle_favorite(server_name: String, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let mut cfg = state.config.lock().unwrap();
    if cfg.favorites.contains(&server_name) {
        cfg.favorites.retain(|f| f != &server_name);
    } else {
        cfg.favorites.push(server_name);
    }
    save_config_to_disk(&cfg);
    Ok(cfg.favorites.clone())
}

#[tauri::command]
async fn vpn_get_control_tower(state: State<'_, AppState>) -> Result<ControlTowerConfig, String> {
    // Try to sync from Portal API if we have a valid token.
    if let Some(token) = get_or_refresh_token(&state).await {
        if let Ok(base) = find_working_api(&state.http, &state.cached_api_base).await {
            let username = state.config.lock().unwrap().username.clone();

            // Fetch block stats from the portal.
            if let Ok(resp) = state.http
                .get(format!("{base}/v1/objects/block_stats/"))
                .query(&[("vpn_username", &username)])
                .bearer_auth(&token)
                .send().await
            {
                if let Ok(data) = resp.json::<serde_json::Value>().await {
                    let mut ct = state.control_tower.lock().unwrap();
                    if let Some(ads) = data["ads_blocked"].as_u64().or(data["data"]["ads_blocked"].as_u64()) {
                        ct.ads_blocked = ads;
                    }
                    if let Some(trackers) = data["trackers_blocked"].as_u64().or(data["data"]["trackers_blocked"].as_u64()) {
                        ct.trackers_blocked = trackers;
                    }
                    if let Some(threats) = data["threats_blocked"].as_u64().or(data["data"]["threats_blocked"].as_u64()) {
                        ct.threats_blocked = threats;
                    }
                    save_json_to_disk("control_tower.json", &*ct);
                }
            }
        }
    }
    Ok(state.control_tower.lock().unwrap().clone())
}

#[tauri::command]
async fn vpn_save_control_tower(config: ControlTowerConfig, state: State<'_, AppState>) -> Result<(), String> {
    *state.control_tower.lock().unwrap() = config.clone();
    save_json_to_disk("control_tower.json", &config);

    // Sync to Portal API if logged in.
    if let Some(token) = get_or_refresh_token(&state).await {
        if let Ok(base) = find_working_api(&state.http, &state.cached_api_base).await {
            let username = state.config.lock().unwrap().username.clone();

            // Push updated profile to portal.
            let body = serde_json::json!({
                "vpn_username": username,
                "ad_blocking": config.ad_blocking,
                "tracker_blocking": config.tracker_blocking,
                "malware_protection": config.malware_protection,
                "phishing_protection": config.phishing_protection,
                "adult_content": config.adult_content,
                "enabled": config.enabled,
            });
            let _ = state.http
                .post(format!("{base}/v1/objects/update_customer/"))
                .bearer_auth(&token)
                .json(&body)
                .send().await;
        }
    }
    Ok(())
}

#[tauri::command]
async fn vpn_get_notifications(state: State<'_, AppState>) -> Result<NotificationConfig, String> {
    Ok(state.notifications.lock().unwrap().clone())
}

#[tauri::command]
async fn vpn_save_notifications(config: NotificationConfig, state: State<'_, AppState>) -> Result<(), String> {
    *state.notifications.lock().unwrap() = config.clone();
    save_json_to_disk("notifications.json", &config);
    Ok(())
}

#[tauri::command]
async fn vpn_add_split_domain(domain: String, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let mut cfg = state.config.lock().unwrap();
    let d = domain.trim().to_lowercase();
    if d.is_empty() { return Ok(cfg.split_domains.clone()); }
    if !cfg.split_domains.contains(&d) {
        cfg.split_domains.push(d);
        cfg.split_tunnel = true;
        save_config_to_disk(&cfg);
    }
    Ok(cfg.split_domains.clone())
}

#[tauri::command]
async fn vpn_remove_split_domain(domain: String, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let mut cfg = state.config.lock().unwrap();
    cfg.split_domains.retain(|d| d != &domain);
    if cfg.split_domains.is_empty() { cfg.split_tunnel = false; }
    save_config_to_disk(&cfg);
    Ok(cfg.split_domains.clone())
}

#[tauri::command]
async fn vpn_import_domains(domains: Vec<String>, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let mut cfg = state.config.lock().unwrap();
    for d in domains {
        let d = d.trim().to_lowercase();
        if !d.is_empty() && !cfg.split_domains.contains(&d) {
            cfg.split_domains.push(d);
        }
    }
    if !cfg.split_domains.is_empty() { cfg.split_tunnel = true; }
    save_config_to_disk(&cfg);
    Ok(cfg.split_domains.clone())
}

#[tauri::command]
async fn vpn_check_connection(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let daemon_status = daemon_get(&state.daemon_http, "/status").await
        .unwrap_or_else(|_| serde_json::json!({"connected": false}));

    let alive = daemon_status["connected"].as_bool().unwrap_or(false);
    let duration = daemon_status["duration_secs"].as_u64().unwrap_or(0);
    let bytes_sent = daemon_status["bytes_out"].as_u64().unwrap_or(0);
    let bytes_recv = daemon_status["bytes_in"].as_u64().unwrap_or(0);

    Ok(serde_json::json!({
        "alive": alive,
        "state": if alive { "Connected" } else { "Disconnected" },
        "duration_secs": duration,
        "bytes_sent": bytes_sent,
        "bytes_recv": bytes_recv,
    }))
}

#[tauri::command]
async fn vpn_reconnect(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    // Disconnect first.
    let _ = daemon_post(&state.daemon_http, "/disconnect", &serde_json::json!({"pin": VPN_PIN})).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Reconnect using preferred country.
    let cfg = state.config.lock().unwrap().clone();
    let country = cfg.preferred_country.clone().unwrap_or_else(|| "NL".into());
    vpn_connect(country, None, state).await
}

#[tauri::command]
async fn vpn_get_ip_info(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    // First try Privado's own ipgeo endpoint (matches APK behavior).
    if let Some(token) = state.get_valid_token() {
        if let Ok(base) = find_working_api(&state.http, &state.cached_api_base).await {
            if let Ok(resp) = state.http
                .post(format!("{base}/v2/ipgeo"))
                .bearer_auth(&token)
                .json(&serde_json::json!({}))
                .send().await
            {
                if let Ok(data) = resp.json::<serde_json::Value>().await {
                    if data.get("ip").is_some() || data.get("data").and_then(|d| d.get("ip")).is_some() {
                        return Ok(data);
                    }
                }
            }
        }
    }

    // Fallback to ipify (always works, no auth needed).
    let resp = state.http.get("https://api.ipify.org?format=json")
        .send().await
        .map_err(|e| format!("IP lookup failed: {e}"))?;
    let data: serde_json::Value = resp.json().await.map_err(|e| format!("Parse: {e}"))?;
    Ok(data)
}

// ====== HELPERS ======

fn resolve_split_routes(domains: &[String]) -> Vec<String> {
    use std::net::ToSocketAddrs;
    let mut routes = Vec::new();
    for domain in domains {
        let lookup = if domain.contains(':') { domain.clone() } else { format!("{domain}:443") };
        if let Ok(addrs) = lookup.to_socket_addrs() {
            for addr in addrs {
                let cidr = if addr.ip().is_ipv6() {
                    format!("{}/128", addr.ip())
                } else {
                    format!("{}/32", addr.ip())
                };
                if !routes.contains(&cidr) { routes.push(cidr); }
            }
        }
    }
    if routes.is_empty() {
        routes.push("0.0.0.0/0".to_string());
    }
    routes
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState::new();

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            vpn_login,
            vpn_logout,
            vpn_get_servers,
            vpn_connect,
            vpn_disconnect,
            vpn_status,
            vpn_get_config,
            vpn_save_config,
            vpn_get_history,
            vpn_clear_history,
            vpn_get_speed_results,
            vpn_run_speed_test,
            vpn_toggle_favorite,
            vpn_get_control_tower,
            vpn_save_control_tower,
            vpn_get_notifications,
            vpn_save_notifications,
            vpn_add_split_domain,
            vpn_remove_split_domain,
            vpn_import_domains,
            vpn_check_connection,
            vpn_reconnect,
            vpn_get_ip_info,
            commands::vpn_ping_servers,
            commands::vpn_create_account,
            commands::vpn_run_speed_test_privado,
            commands::vpn_get_control_tower_full,
            commands::vpn_save_control_tower_profile,
            commands::vpn_run_diagnostics,
            commands::vpn_check_breach,
            commands::vpn_security_scan,
            commands::vpn_manage_subscription,
            commands::vpn_add_split_process,
            commands::vpn_remove_split_process,
            commands::vpn_list_split_processes,
            commands::vpn_pause_connection,
            commands::vpn_send_notification,
            commands::vpn_report_error,
            commands::vpn_track_event,
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running PrivadoVPN");
}
