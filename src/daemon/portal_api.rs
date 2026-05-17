//! Privado Portal API client for the daemon process.
//!
//! The daemon runs as root and reads the user's saved token from
//! ~/.config/privado-vpn/token.json. It uses this to fetch the real server
//! list from Privado's API, replacing the old hardcoded 3-server approach.
//!
//! Server list is cached in memory with a 5-minute TTL to avoid hammering the API.

use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::{info, warn};

const API_KEY: &str = "9f994c466340e8f2ed60a99396fecb6a";
const API_SERVERS: &[&str] = &[
    "https://f3556fm3o524m9.com",
    "https://3nkh5crxol.ch:15748",
    "https://qya97ge69i2loo.com:7491",
    "https://tsgqi2p2na3m7q.net:35486",
    "https://monoprivacy.io:47654",
    "https://netdefenderpro.com:14358",
    "https://client-api.privado.live:59142",
    "https://client-api.prvd.info:24865",
];

const CACHE_TTL_SECS: u64 = 300;

/// A server entry from the Privado Portal API.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PortalServer {
    pub hostname: String,
    pub name: String,
    pub city: String,
    pub country: String,
    pub country_code: String,
    pub ip: String,
    pub load: f64,
    pub status: String,
}

/// Cached server list with TTL.
struct ServerCache {
    servers: Vec<PortalServer>,
    fetched_at: Option<Instant>,
}

static SERVER_CACHE: std::sync::LazyLock<Mutex<ServerCache>> = std::sync::LazyLock::new(|| {
    Mutex::new(ServerCache { servers: Vec::new(), fetched_at: None })
});

/// Get the full server list, fetching from the API if cache is stale.
pub async fn get_servers() -> Vec<PortalServer> {
    // Check cache first.
    {
        let cache = SERVER_CACHE.lock().unwrap();
        if let Some(fetched) = cache.fetched_at {
            if fetched.elapsed() < Duration::from_secs(CACHE_TTL_SECS) && !cache.servers.is_empty() {
                return cache.servers.clone();
            }
        }
    }

    // Cache is stale or empty — fetch from API.
    match fetch_servers_from_api().await {
        Ok(servers) => {
            let mut cache = SERVER_CACHE.lock().unwrap();
            cache.servers = servers.clone();
            cache.fetched_at = Some(Instant::now());
            info!("[portal-api] fetched {} servers", servers.len());
            servers
        }
        Err(e) => {
            warn!("[portal-api] fetch failed: {e}, using cache or fallback");
            let cache = SERVER_CACHE.lock().unwrap();
            if !cache.servers.is_empty() {
                return cache.servers.clone();
            }
            // Final fallback: return a minimal hardcoded list so connections aren't impossible.
            fallback_servers()
        }
    }
}

/// Find the best server for a given country code (lowest load, online).
pub async fn find_server_for_country(country_code: &str) -> Option<PortalServer> {
    let servers = get_servers().await;
    let cc = country_code.to_uppercase();

    let mut candidates: Vec<&PortalServer> = servers.iter()
        .filter(|s| s.country_code.eq_ignore_ascii_case(&cc))
        .filter(|s| s.status == "online" || s.status == "1" || s.status == "active" || s.status.is_empty())
        .collect();

    if candidates.is_empty() {
        // Try matching without status filter.
        candidates = servers.iter()
            .filter(|s| s.country_code.eq_ignore_ascii_case(&cc))
            .collect();
    }

    candidates.sort_by(|a, b| a.load.partial_cmp(&b.load).unwrap_or(std::cmp::Ordering::Equal));
    candidates.first().cloned().cloned()
}

/// Fetch the full server list from the Privado Portal API.
async fn fetch_servers_from_api() -> Result<Vec<PortalServer>, String> {
    let token = read_saved_token()?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(10))
        .user_agent("PrivadoVPN-Linux/2.0.0")
        .build()
        .map_err(|e| format!("HTTP client: {e}"))?;

    // Try each API server until one responds.
    for base in API_SERVERS {
        let resp = match client.get(format!("{base}/v2/servers"))
            .query(&[("nodes", "all"), ("language", "en")])
            .bearer_auth(&token)
            .send().await
        {
            Ok(r) => r,
            Err(_) => continue,
        };

        if !resp.status().is_success() { continue; }

        let text = match resp.text().await {
            Ok(t) => t,
            Err(_) => continue,
        };

        let raw: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Parse the server list from the API response.
        // Handles: { data: { servers: [...] } }, { servers: [...] }, or direct array.
        let list = extract_server_array(&raw);
        if list.is_empty() { continue; }

        let servers: Vec<PortalServer> = list.iter().filter_map(|v| {
            let hostname = v["hostname"].as_str()
                .or(v["dns"].as_str())
                .or(v["ip"].as_str())?;
            if hostname.is_empty() { return None; }

            Some(PortalServer {
                hostname: hostname.to_string(),
                name: v["name"].as_str().unwrap_or(hostname).to_string(),
                city: v["city"].as_str().unwrap_or("").to_string(),
                country: v["country"].as_str().unwrap_or("").to_string(),
                country_code: v["country_code"].as_str()
                    .or(v["cc"].as_str())
                    .unwrap_or("")
                    .to_uppercase(),
                ip: v["ip"].as_str().unwrap_or("").to_string(),
                load: v["load"].as_f64()
                    .or(v["current_load"].as_f64())
                    .unwrap_or(0.0),
                status: v["status"].as_str().unwrap_or("online").to_string(),
            })
        }).collect();

        if !servers.is_empty() {
            return Ok(servers);
        }
    }

    Err("All API endpoints failed".into())
}

/// Extract the server array from various API response formats.
fn extract_server_array(raw: &serde_json::Value) -> Vec<serde_json::Value> {
    if let Some(arr) = raw.as_array() {
        return arr.clone();
    }
    if let Some(obj) = raw.as_object() {
        if let Some(data) = obj.get("data") {
            if let Some(servers) = data.get("servers").and_then(|v| v.as_array()) {
                return servers.clone();
            }
            if let Some(arr) = data.as_array() {
                return arr.clone();
            }
        }
        if let Some(arr) = obj.get("servers").and_then(|v| v.as_array()) {
            return arr.clone();
        }
        if let Some(arr) = obj.get("nodes").and_then(|v| v.as_array()) {
            return arr.clone();
        }
    }
    Vec::new()
}

/// Read the saved API token from the user's config directory.
fn read_saved_token() -> Result<String, String> {
    let token_path = crate::config::config_dir().join("token.json");
    let text = std::fs::read_to_string(&token_path)
        .map_err(|e| format!("read token.json: {e}"))?;
    let data: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("parse token.json: {e}"))?;

    let token = data["access_token"].as_str()
        .ok_or("no access_token in token.json")?;

    // Check expiry if present.
    if let Some(expires) = data["expires_at"].as_u64() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now >= expires {
            // Token expired — try to re-login with saved credentials.
            return refresh_token_via_login();
        }
    }

    Ok(token.to_string())
}

/// Re-login using saved credentials to get a fresh token.
fn refresh_token_via_login() -> Result<String, String> {
    let (username, password) = crate::config::load_credentials()
        .ok_or("no saved credentials for token refresh")?;

    // Synchronous HTTP request (called from sync context inside LazyLock).
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("PrivadoVPN-Linux/2.0.0")
        .build()
        .map_err(|e| format!("blocking client: {e}"))?;

    for base in API_SERVERS {
        let body = serde_json::json!({
            "api_key": API_KEY,
            "username": username,
            "password": password,
            "language": "en",
        });

        let resp = match client.post(format!("{base}/v1/login")).json(&body).send() {
            Ok(r) => r,
            Err(_) => continue,
        };

        if !resp.status().is_success() { continue; }

        let text = match resp.text() {
            Ok(t) => t,
            Err(_) => continue,
        };

        let data: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if let Some(tok) = data["access_token"].as_str() {
            // Save the refreshed token.
            let token_path = crate::config::config_dir().join("token.json");
            let _ = std::fs::write(&token_path, &text);
            return Ok(tok.to_string());
        }
    }

    Err("re-login failed on all API servers".into())
}

/// Fallback server list when API is unreachable and cache is empty.
fn fallback_servers() -> Vec<PortalServer> {
    let entries = [
        ("ams-101.vpn.privado.io", "Amsterdam", "Netherlands", "NL"),
        ("sin-005.vpn.privado.io", "Singapore", "Singapore", "SG"),
        ("mex-011.vpn.privado.io", "Mexico City", "Mexico", "MX"),
        ("lon-101.vpn.privado.io", "London", "United Kingdom", "GB"),
        ("fra-101.vpn.privado.io", "Frankfurt", "Germany", "DE"),
        ("nyc-101.vpn.privado.io", "New York", "United States", "US"),
        ("lax-101.vpn.privado.io", "Los Angeles", "United States", "US"),
        ("tor-101.vpn.privado.io", "Toronto", "Canada", "CA"),
        ("tok-101.vpn.privado.io", "Tokyo", "Japan", "JP"),
        ("syd-101.vpn.privado.io", "Sydney", "Australia", "AU"),
        ("par-101.vpn.privado.io", "Paris", "France", "FR"),
        ("zrh-101.vpn.privado.io", "Zurich", "Switzerland", "CH"),
    ];
    entries.iter().map(|(host, city, country, cc)| PortalServer {
        hostname: host.to_string(),
        name: format!("{city} ({cc})"),
        city: city.to_string(),
        country: country.to_string(),
        country_code: cc.to_string(),
        ip: String::new(),
        load: 0.0,
        status: "online".to_string(),
    }).collect()
}
