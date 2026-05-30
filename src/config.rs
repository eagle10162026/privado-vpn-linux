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
    /// Per-traffic routing rules (process / domain / ip-cidr / port) that map
    /// onto the VPN fwmark engine. Authoritative over the legacy split_domains
    /// list (which is kept as a read-only mirror for the connect-time
    /// remote_ts selector). New configs start empty.
    #[serde(default)]
    pub routing_rules: Vec<RoutingRule>,
}

/// What a routing rule matches on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RuleMatchType {
    /// A process: `uid:1000` or an app/comm name like `firefox`.
    Process,
    /// A fully-qualified domain name (resolved to IPs at apply time).
    Domain,
    /// A CIDR, e.g. `1.2.3.0/24` or `2001:db8::/32`.
    IpCidr,
    /// A single port, e.g. `443`.
    Port,
    /// A port range, e.g. `8000-8100`.
    PortRange,
}

/// What a routing rule does with matched traffic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    /// Send matched traffic through the VPN tunnel (fwmark 0x1016).
    Vpn,
    /// Force matched traffic to bypass the VPN (mark 0x0, short-circuit).
    Direct,
}

/// A single routing rule. Rules are evaluated in `priority` order (ascending);
/// the daemon keeps the on-disk array order equal to priority order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    /// Stable id; the daemon generates one if empty on add.
    pub id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub name: String,
    pub match_type: RuleMatchType,
    /// process: comm/exe name or `uid:<n>`; domain: fqdn; ip_cidr: `1.2.3.0/24`;
    /// port: `443`; port_range: `8000-8100`.
    pub match_value: String,
    /// `tcp` | `udp` | None (=both). Only meaningful for port/port_range.
    #[serde(default)]
    pub protocol: Option<String>,
    pub action: RuleAction,
    /// Preferred exit server hostname OR a selector (`cc:us` / `city:...`);
    /// None = use whatever tunnel is currently active.
    #[serde(default)]
    pub exit_server: Option<String>,
    /// Lower = evaluated first. Daemon keeps the array order == priority order.
    #[serde(default)]
    pub priority: u32,
}

fn default_true() -> bool { true }

/// Generate a short, time-seeded pseudo-random id for a new routing rule.
/// Not cryptographic — uniqueness within one config is all that's needed.
pub fn gen_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    // xorshift-mix the nanosecond clock into a 64-bit value, then hex-encode.
    let mut x = (nanos as u64) ^ 0x9E37_79B9_7F4A_7C15;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    format!("rule-{x:016x}")
}

/// One-shot migration: turn a legacy split_tunnel + split_domains config into
/// equivalent `Domain`/`Vpn` routing rules (named `legacy:<domain>`). Idempotent
/// — does nothing if legacy rules already exist. Returns true if it mutated cfg.
pub fn migrate_split_domains_to_rules(cfg: &mut Config) -> bool {
    if cfg.split_tunnel
        && !cfg.split_domains.is_empty()
        && !cfg.routing_rules.iter().any(|r| r.name.starts_with("legacy:"))
    {
        for (i, d) in cfg.split_domains.iter().enumerate() {
            cfg.routing_rules.push(RoutingRule {
                id: gen_id(),
                enabled: true,
                name: format!("legacy:{d}"),
                match_type: RuleMatchType::Domain,
                match_value: d.clone(),
                protocol: None,
                action: RuleAction::Vpn,
                exit_server: None,
                priority: 100 + i as u32,
            });
        }
        return true;
    }
    false
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
            routing_rules: Vec::new(),
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
