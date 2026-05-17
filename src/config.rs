use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const CONFIG_DIR_NAME: &str = "privado-vpn";
const CONFIG_FILE_NAME: &str = "config.json";
const TOKEN_FILE_NAME: &str = "token.json";
const CREDS_FILE_NAME: &str = "credentials.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub username: String,
    #[serde(default, skip_serializing)]
    pub password: String,
    pub preferred_country: Option<String>,
    pub preferred_city: Option<String>,
    pub split_tunnel: bool,
    pub split_domains: Vec<String>,
    pub kill_switch: bool,
    pub auto_connect: bool,
    pub dns_servers: Vec<String>,
    /// WiFi SSIDs where the VPN should auto-disconnect (trusted = no VPN needed).
    #[serde(default)]
    pub trusted_networks: Vec<String>,
    /// Preferred VPN protocol: "ikev2", "wireguard", or "openvpn".
    #[serde(default = "default_protocol")]
    pub protocol: String,
    /// Protocol preference order for auto-switching on failure.
    #[serde(default = "default_protocol_preference")]
    pub protocol_preference: Vec<String>,
    /// Toggle: route LLM browser (Stygian) through the VPN when connected.
    /// Only active when VPN is connected AND this toggle is on.
    #[serde(default)]
    pub route_llm_browser: bool,
    /// Toggle: route LLM tool network traffic through the VPN when connected.
    /// Only active when VPN is connected AND this toggle is on.
    /// Tools remain fully accessible regardless — this only affects routing.
    #[serde(default)]
    pub route_llm_tools: bool,
}

fn default_protocol() -> String { "ikev2".to_string() }
fn default_protocol_preference() -> Vec<String> {
    vec!["ikev2".into(), "wireguard".into(), "openvpn".into()]
}

impl Default for Config {
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
            trusted_networks: Vec::new(),
            protocol: default_protocol(),
            protocol_preference: default_protocol_preference(),
            route_llm_browser: false,
            route_llm_tools: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: u64,
}

pub fn config_dir() -> PathBuf {
    // When running as the daemon (root via systemd), dirs::config_dir()
    // returns /root/.config/ — but the user's config lives in their home
    // directory. Check for the real user's XDG config first.
    if let Ok(uid) = std::env::var("SUDO_UID").or_else(|_| std::env::var("PKEXEC_UID")) {
        // Running under sudo/pkexec — use the real user's home.
        if let Ok(user) = std::process::Command::new("getent")
            .args(["passwd", &uid])
            .output()
        {
            let entry = String::from_utf8_lossy(&user.stdout);
            if let Some(home) = entry.split(':').nth(5) {
                let path = PathBuf::from(home).join(".config").join(CONFIG_DIR_NAME);
                if path.exists() { return path; }
            }
        }
    }

    // When running as root daemon, resolve via SUDO_USER or scan /home.
    if let Ok(user) = std::env::var("SUDO_USER") {
        if let Ok(out) = std::process::Command::new("getent")
            .args(["passwd", &user])
            .output()
        {
            let entry = String::from_utf8_lossy(&out.stdout);
            if let Some(home) = entry.split(':').nth(5) {
                let path = PathBuf::from(home).join(".config").join(CONFIG_DIR_NAME);
                if path.exists() { return path; }
            }
        }
    }

    // Fallback: scan /home for any user who has our config dir (single-user systems).
    if std::env::var("USER").as_deref() == Ok("root")
        || std::env::var("HOME").as_deref() == Ok("/root")
    {
        if let Ok(entries) = std::fs::read_dir("/home") {
            for entry in entries.flatten() {
                let candidate = entry.path().join(".config").join(CONFIG_DIR_NAME);
                if candidate.exists() {
                    return candidate;
                }
            }
        }
    }

    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/etc"))
        .join(CONFIG_DIR_NAME)
}

pub fn config_path() -> PathBuf {
    config_dir().join(CONFIG_FILE_NAME)
}

fn token_path() -> PathBuf {
    config_dir().join(TOKEN_FILE_NAME)
}

pub fn load_config() -> Option<Config> {
    let path = config_path();
    let text = fs::read_to_string(&path).ok()?;
    let mut cfg: Config = serde_json::from_str(&text).ok()?;
    if let Some((user, pass)) = load_credentials() {
        cfg.username = user;
        cfg.password = pass;
    }
    Some(cfg)
}

pub fn save_config(config: &Config) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("create config dir: {e}"))?;
    let path = dir.join(CONFIG_FILE_NAME);

    let mut save_copy = config.clone();
    save_copy.password = String::new();

    let text = serde_json::to_string_pretty(&save_copy)
        .map_err(|e| format!("serialize config: {e}"))?;
    fs::write(&path, &text).map_err(|e| format!("write config: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        fs::set_permissions(&path, perms).ok();
    }

    Ok(())
}

pub fn save_token(token: &SavedToken) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("create config dir: {e}"))?;
    let path = token_path();
    let text = serde_json::to_string_pretty(token)
        .map_err(|e| format!("serialize token: {e}"))?;
    fs::write(&path, &text).map_err(|e| format!("write token: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        fs::set_permissions(&path, perms).ok();
    }

    Ok(())
}

#[allow(dead_code)]
pub fn load_token() -> Option<SavedToken> {
    let path = token_path();
    let text = fs::read_to_string(&path).ok()?;
    let token: SavedToken = serde_json::from_str(&text).ok()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if token.expires_at > now {
        Some(token)
    } else {
        None
    }
}

fn creds_path() -> PathBuf {
    config_dir().join(CREDS_FILE_NAME)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SavedCredentials {
    username: String,
    password: String,
}

pub fn save_credentials(username: &str, password: &str) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("create config dir: {e}"))?;
    let creds = SavedCredentials { username: username.into(), password: password.into() };
    let text = serde_json::to_string_pretty(&creds).map_err(|e| format!("serialize: {e}"))?;
    let path = creds_path();
    fs::write(&path, &text).map_err(|e| format!("write creds: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).ok();
    }
    Ok(())
}

pub fn load_credentials() -> Option<(String, String)> {
    let text = fs::read_to_string(creds_path()).ok()?;
    let creds: SavedCredentials = serde_json::from_str(&text).ok()?;
    if creds.username.is_empty() { return None; }
    Some((creds.username, creds.password))
}

pub fn clear_credentials() {
    let _ = fs::remove_file(token_path());
    let _ = fs::remove_file(creds_path());
}
