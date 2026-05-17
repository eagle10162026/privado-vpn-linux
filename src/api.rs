use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, warn};

const API_KEY: &str = "9f994c466340e8f2ed60a99396fecb6a";
const USER_AGENT: &str = "PrivadoVPN-Linux/1.0.0 (Rust; x86_64-linux-gnu)";

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

#[derive(Debug, Clone, Serialize)]
struct LoginRequest {
    api_key: String,
    username: String,
    password: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct LoginResponse {
    pub access_token: Option<String>,
    pub token_type: Option<String>,
    pub expires_in: Option<u64>,
    pub access_expire_epoch: Option<u64>,
    pub refresh_token: Option<String>,
    pub username: Option<String>,
    pub account_type: Option<i32>,
    #[serde(default)]
    pub error: Option<String>,
}

impl LoginResponse {
    pub fn expires_at(&self) -> u64 {
        if let Some(epoch) = self.access_expire_epoch {
            return epoch;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now + self.expires_in.unwrap_or(86400)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ServerEntry {
    pub id: Option<u64>,
    pub name: Option<String>,
    pub hostname: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub country_code: Option<String>,
    pub ip: Option<String>,
    pub status: Option<String>,
    pub tier: Option<String>,
    #[serde(default)]
    pub load: Option<f64>,
    #[serde(default)]
    pub geo: Option<bool>,
}

#[allow(dead_code)]
impl ServerEntry {
    pub fn display_name(&self) -> String {
        let city = self.city.as_deref().unwrap_or("Unknown");
        let country = self.country.as_deref().unwrap_or("Unknown");
        let name = self.name.as_deref().unwrap_or("unknown");
        format!("{city}, {country} ({name})")
    }

    pub fn connect_host(&self) -> &str {
        self.hostname
            .as_deref()
            .or(self.ip.as_deref())
            .or(self.name.as_deref())
            .unwrap_or("unknown")
    }

    pub fn is_available(&self) -> bool {
        self.status.as_deref().map(|s| s == "online" || s == "1").unwrap_or(true)
    }
}


pub struct PrivadoApi {
    client: Client,
    token: Option<String>,
}

impl PrivadoApi {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent(USER_AGENT)
            .danger_accept_invalid_certs(false)
            .build()
            .expect("failed to build HTTP client");
        Self { client, token: None }
    }

    async fn find_working_api(&self) -> Option<String> {
        for server in API_SERVERS {
            let url = if server.contains("://") {
                server.to_string()
            } else {
                format!("https://{server}")
            };
            match self.client.get(format!("{url}/v1/status")).send().await {
                Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 404 => {
                    info!("[API] Using endpoint: {url}");
                    return Some(url);
                }
                Ok(resp) => {
                    warn!("[API] {server} returned {}", resp.status());
                }
                Err(e) => {
                    warn!("[API] {server} unreachable: {e}");
                }
            }
        }
        None
    }

    pub async fn login(&mut self, username: &str, password: &str) -> Result<LoginResponse, String> {
        let base = self.find_working_api().await
            .ok_or("all Privado API endpoints are unreachable")?;

        let body = LoginRequest {
            api_key: API_KEY.to_string(),
            username: username.to_string(),
            password: password.to_string(),
        };

        let resp = self.client
            .post(format!("{base}/v1/login"))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("login request failed: {e}"))?;

        let status = resp.status();
        let body = resp.text().await
            .map_err(|e| format!("login response read failed: {e}"))?;
        let data: LoginResponse = serde_json::from_str(&body)
            .map_err(|e| format!("login response parse failed: {e} — body: {}", &body[..body.len().min(200)]))?;

        if let Some(ref err) = data.error {
            return Err(format!("login failed ({}): {err}", status));
        }

        if let Some(ref token) = data.access_token {
            self.token = Some(token.clone());
            info!("[API] Login successful — token expires in {}s", data.expires_in.unwrap_or(0));
        } else {
            return Err(format!("login response missing access_token (status {status})"));
        }

        Ok(data)
    }

    #[allow(dead_code)]
    pub async fn get_servers(&self) -> Result<Vec<ServerEntry>, String> {
        let token = self.token.as_ref().ok_or("not logged in")?;
        let base = self.find_working_api().await
            .ok_or("all Privado API endpoints are unreachable")?;

        let resp = self.client
            .get(format!("{base}/v1/servers?nodes=all&includegeo=1"))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("servers request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("servers request returned {}", resp.status()));
        }

        let text = resp.text().await.map_err(|e| format!("servers body read: {e}"))?;

        if let Ok(list) = serde_json::from_str::<Vec<ServerEntry>>(&text) {
            return Ok(list);
        }

        if let Ok(map) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(servers) = map.get("servers").or(map.get("data")).or(map.get("nodes")) {
                if let Ok(list) = serde_json::from_value::<Vec<ServerEntry>>(servers.clone()) {
                    return Ok(list);
                }
            }
            if let Some(obj) = map.as_object() {
                let mut entries = Vec::new();
                for (key, val) in obj {
                    if let Ok(mut entry) = serde_json::from_value::<ServerEntry>(val.clone()) {
                        if entry.name.is_none() {
                            entry.name = Some(key.clone());
                        }
                        entries.push(entry);
                    }
                }
                if !entries.is_empty() {
                    return Ok(entries);
                }
            }
        }

        Err("failed to parse server list from any known format".into())
    }

    /// Inject an existing bearer token (e.g. from config::load_token()) so
    /// get_servers() can work without re-logging in.
    #[allow(dead_code)]
    pub fn set_token(&mut self, token: String) {
        self.token = Some(token);
    }

    #[allow(dead_code)]
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }
}
