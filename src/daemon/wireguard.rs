//! WireGuard protocol support for PrivadoVPN.
//!
//! Flow (reverse-engineered from APK `WireGuardManager.java`):
//! 1. POST /v2/wireguard/login with { username, password, server_socket } → get WG config
//! 2. Write wg0.conf with returned private key, endpoint, allowed IPs
//! 3. Call `wg-quick up wg0` to bring up the interface
//! 4. On disconnect: `wg-quick down wg0` + POST /v2/wireguard/logout

use tokio::process::Command;
use tracing::{info, warn};

const WG_CONF_PATH: &str = "/etc/wireguard/privado-wg0.conf";
const WG_INTERFACE: &str = "privado-wg0";

/// WireGuard connection parameters returned by the Privado API.
#[derive(Debug, Clone)]
pub struct WgConfig {
    pub private_key: String,
    pub address: String,
    pub dns: Vec<String>,
    pub endpoint: String,
    pub public_key: String,
    pub allowed_ips: Vec<String>,
    pub preshared_key: Option<String>,
}

/// Authenticate with Privado's WireGuard API and retrieve connection parameters.
/// Calls POST /v2/wireguard/login on the portal API.
///
/// Handles all known response format variations from Privado's API:
/// - Nested under "data" key or flat at top level
/// - camelCase (privateKey) and snake_case (private_key) field names
/// - Array vs comma-separated string for allowed_ips
/// - Various address formats (with/without CIDR suffix)
pub async fn wg_login(
    api_base: &str,
    token: &str,
    username: &str,
    password: &str,
    server_host: &str,
) -> Result<WgConfig, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .danger_accept_invalid_certs(false)
        .build()
        .map_err(|e| format!("HTTP client: {e}"))?;

    // The APK sends the server socket as "hostname:51820" (WG default port).
    let server_socket = if server_host.contains(':') {
        server_host.to_string()
    } else {
        format!("{server_host}:51820")
    };

    // Try multiple request body formats — different API versions accept different shapes.
    let bodies = [
        serde_json::json!({
            "username": username,
            "password": password,
            "server_socket": server_socket,
        }),
        serde_json::json!({
            "username": username,
            "password": password,
            "server": server_host,
            "port": 51820,
        }),
        serde_json::json!({
            "vpn_username": username,
            "vpn_password": password,
            "server_socket": server_socket,
        }),
    ];

    let mut last_error = String::from("all request formats failed");

    for body in &bodies {
        let resp = match client.post(format!("{api_base}/v2/wireguard/login"))
            .bearer_auth(token)
            .json(body)
            .send().await
        {
            Ok(r) => r,
            Err(e) => {
                last_error = format!("WG login request: {e}");
                continue;
            }
        };

        let status = resp.status();
        let text = match resp.text().await {
            Ok(t) => t,
            Err(e) => {
                last_error = format!("WG login read: {e}");
                continue;
            }
        };

        if !status.is_success() {
            last_error = format!("WG login {status}: {text}");
            continue;
        }

        let data: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                last_error = format!("WG login parse: {e}");
                continue;
            }
        };

        // Try to extract config from multiple possible response structures.
        match extract_wg_config(&data, &server_socket) {
            Ok(cfg) => return Ok(cfg),
            Err(e) => {
                last_error = e;
                continue;
            }
        }
    }

    Err(last_error)
}

/// Extract WireGuard config from API response JSON, handling all known formats.
fn extract_wg_config(data: &serde_json::Value, fallback_endpoint: &str) -> Result<WgConfig, String> {
    // Response may nest under "data", "result", "config", or be flat.
    let candidates = [
        data.get("data"),
        data.get("result"),
        data.get("config"),
        data.get("wireguard"),
        Some(data),
    ];

    for candidate in candidates.into_iter().flatten() {
        let d = candidate;

        // Private key: try every known field name.
        let private_key = json_str(d, &[
            "private_key", "privateKey", "PrivateKey",
            "client_private_key", "clientPrivateKey", "wg_private_key",
        ]);

        // Public key: server's public key for the peer section.
        let public_key = json_str(d, &[
            "public_key", "publicKey", "PublicKey",
            "server_public_key", "serverPublicKey", "peer_public_key",
        ]);

        // Both are required — skip this candidate if missing.
        let private_key = match private_key {
            Some(k) if !k.is_empty() && k.len() >= 32 => k,
            _ => continue,
        };
        let public_key = match public_key {
            Some(k) if !k.is_empty() && k.len() >= 32 => k,
            _ => continue,
        };

        // Address: client's tunnel IP.
        let address = json_str(d, &[
            "address", "Address", "client_ip", "clientIp",
            "internal_ip", "internalIp", "ip", "tunnel_ip",
        ]).unwrap_or_else(|| "10.0.0.2/32".to_string());

        // Ensure address has CIDR notation.
        let address = if address.contains('/') {
            address
        } else {
            format!("{address}/32")
        };

        // Endpoint: server's WG endpoint (ip:port).
        let endpoint = json_str(d, &[
            "endpoint", "Endpoint", "server_endpoint", "serverEndpoint",
            "peer_endpoint", "server_address",
        ]).unwrap_or_else(|| fallback_endpoint.to_string());

        // Allowed IPs: may be string or array.
        let allowed_ips = extract_string_list(d, &[
            "allowed_ips", "allowedIPs", "AllowedIPs", "allowedIps",
            "allowed_ips_list", "routes",
        ], "0.0.0.0/0, ::/0");

        // DNS servers: may be string or array.
        let dns = extract_string_list(d, &[
            "dns", "DNS", "dns_servers", "dnsServers",
        ], "198.18.0.1");

        // Preshared key (optional).
        let preshared_key = json_str(d, &[
            "preshared_key", "presharedKey", "PresharedKey", "pre_shared_key",
        ]).filter(|s| !s.is_empty() && s.len() >= 32);

        return Ok(WgConfig {
            private_key,
            address,
            dns,
            endpoint,
            public_key,
            allowed_ips,
            preshared_key,
        });
    }

    Err("Could not extract WireGuard config from any known response format".into())
}

/// Try multiple field names and return the first non-null string value.
fn json_str(obj: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(val) = obj.get(*key) {
            if let Some(s) = val.as_str() {
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            }
        }
    }
    None
}

/// Extract a list of strings from a JSON value that may be either a
/// comma-separated string or a JSON array of strings.
fn extract_string_list(obj: &serde_json::Value, keys: &[&str], default: &str) -> Vec<String> {
    for key in keys {
        if let Some(val) = obj.get(*key) {
            if let Some(arr) = val.as_array() {
                let items: Vec<String> = arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .collect();
                if !items.is_empty() { return items; }
            }
            if let Some(s) = val.as_str() {
                let items: Vec<String> = s.split(',')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect();
                if !items.is_empty() { return items; }
            }
        }
    }
    default.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// Write the WireGuard config file and bring up the interface.
pub async fn connect(config: &WgConfig) -> Result<(), String> {
    // Ensure /etc/wireguard/ exists.
    tokio::fs::create_dir_all("/etc/wireguard/").await
        .map_err(|e| format!("create /etc/wireguard/: {e}"))?;

    // Generate wg-quick compatible config.
    let mut conf = format!(
        "[Interface]\nPrivateKey = {}\nAddress = {}\n",
        config.private_key, config.address
    );

    if !config.dns.is_empty() {
        conf.push_str(&format!("DNS = {}\n", config.dns.join(", ")));
    }

    // Add routing table and fwmark so per-process split tunnel works with WG.
    // wg-quick uses Table, FwMark, and PostUp/PreDown to integrate with iptables.
    // This ensures UID-based bypass rules (from vpn_add_split_process) take effect.
    conf.push_str("Table = 1234\n");
    conf.push_str("FwMark = 0x1234\n");

    // PostUp: add ip rule to route marked traffic through WG, and allow
    // unmarked traffic (UID-bypassed processes) to skip the tunnel.
    conf.push_str("PostUp = ip rule add not fwmark 0x1234 table 1234 priority 100\n");
    conf.push_str("PostUp = ip rule add table main suppress_prefixlength 0 priority 50\n");
    conf.push_str("PreDown = ip rule del not fwmark 0x1234 table 1234 priority 100\n");
    conf.push_str("PreDown = ip rule del table main suppress_prefixlength 0 priority 50\n");

    conf.push_str("\n[Peer]\n");
    conf.push_str(&format!("PublicKey = {}\n", config.public_key));

    if let Some(ref psk) = config.preshared_key {
        if !psk.is_empty() {
            conf.push_str(&format!("PresharedKey = {}\n", psk));
        }
    }

    conf.push_str(&format!("Endpoint = {}\n", config.endpoint));
    conf.push_str(&format!("AllowedIPs = {}\n", config.allowed_ips.join(", ")));
    conf.push_str("PersistentKeepalive = 25\n");

    // Write config file with restrictive permissions.
    tokio::fs::write(WG_CONF_PATH, &conf).await
        .map_err(|e| format!("write WG config: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = tokio::fs::set_permissions(
            WG_CONF_PATH,
            std::fs::Permissions::from_mode(0o600),
        ).await;
    }

    info!("[wireguard] config written to {WG_CONF_PATH}");

    // Bring down any existing instance first (ignore error if not up).
    let _ = Command::new("wg-quick")
        .args(["down", WG_INTERFACE])
        .output().await;

    // Bring up the WireGuard interface.
    let up = Command::new("wg-quick")
        .args(["up", WG_CONF_PATH])
        .output().await
        .map_err(|e| format!("wg-quick up: {e}"))?;

    if !up.status.success() {
        let stderr = String::from_utf8_lossy(&up.stderr);
        return Err(format!("wg-quick up failed: {stderr}"));
    }

    info!("[wireguard] interface {WG_INTERFACE} is up");
    Ok(())
}

/// Tear down the WireGuard interface and clean up config.
pub async fn disconnect() -> Result<(), String> {
    let down = Command::new("wg-quick")
        .args(["down", WG_CONF_PATH])
        .output().await
        .map_err(|e| format!("wg-quick down: {e}"))?;

    if !down.status.success() {
        let stderr = String::from_utf8_lossy(&down.stderr);
        warn!("[wireguard] wg-quick down warning: {stderr}");
    }

    // Remove the config file.
    let _ = tokio::fs::remove_file(WG_CONF_PATH).await;

    info!("[wireguard] disconnected and cleaned up");
    Ok(())
}

/// Call Privado's WireGuard logout API to release the server-side session.
pub async fn wg_logout(api_base: &str, token: &str, username: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client: {e}"))?;

    let body = serde_json::json!({ "username": username });

    let resp = client.post(format!("{api_base}/v2/wireguard/logout"))
        .bearer_auth(token)
        .json(&body)
        .send().await
        .map_err(|e| format!("WG logout: {e}"))?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        warn!("[wireguard] logout returned non-200: {text}");
    }

    Ok(())
}

/// Check if WireGuard tools are available on the system.
pub async fn is_available() -> bool {
    Command::new("which")
        .arg("wg-quick")
        .output().await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get the status of the WireGuard interface (if active).
pub async fn status() -> Option<WgStatus> {
    let output = Command::new("wg")
        .args(["show", WG_INTERFACE])
        .output().await.ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut endpoint = String::new();
    let mut transfer_rx: u64 = 0;
    let mut transfer_tx: u64 = 0;
    let mut latest_handshake: u64 = 0;

    for line in text.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("endpoint:") {
            endpoint = val.trim().to_string();
        }
        if let Some(val) = line.strip_prefix("transfer:") {
            // "1.23 MiB received, 456.78 KiB sent"
            let parts: Vec<&str> = val.split(',').collect();
            if let Some(rx_part) = parts.first() {
                transfer_rx = parse_transfer_bytes(rx_part.trim());
            }
            if let Some(tx_part) = parts.get(1) {
                transfer_tx = parse_transfer_bytes(tx_part.trim());
            }
        }
        if let Some(val) = line.strip_prefix("latest handshake:") {
            latest_handshake = parse_wg_time(val.trim());
        }
    }

    Some(WgStatus {
        connected: true,
        endpoint,
        bytes_rx: transfer_rx,
        bytes_tx: transfer_tx,
        last_handshake_secs_ago: latest_handshake,
    })
}

#[derive(Debug, Clone)]
pub struct WgStatus {
    pub connected: bool,
    pub endpoint: String,
    pub bytes_rx: u64,
    pub bytes_tx: u64,
    pub last_handshake_secs_ago: u64,
}

/// Parse a transfer string like "1.23 MiB" or "456 KiB" into bytes.
fn parse_transfer_bytes(s: &str) -> u64 {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 2 { return 0; }

    let num: f64 = parts[0].parse().unwrap_or(0.0);
    let unit = parts[1].to_lowercase();

    match unit.as_str() {
        "b" | "bytes" => num as u64,
        "kib" | "kb" => (num * 1024.0) as u64,
        "mib" | "mb" => (num * 1024.0 * 1024.0) as u64,
        "gib" | "gb" => (num * 1024.0 * 1024.0 * 1024.0) as u64,
        _ => num as u64,
    }
}

/// Parse WireGuard time strings like "1 minute, 30 seconds ago" into seconds.
fn parse_wg_time(s: &str) -> u64 {
    let s = s.trim_end_matches(" ago").trim();
    let mut total: u64 = 0;
    let mut current_num: u64 = 0;

    for part in s.split(|c: char| c == ',' || c == ' ') {
        let part = part.trim();
        if part.is_empty() { continue; }

        if let Ok(n) = part.parse::<u64>() {
            current_num = n;
        } else if part.starts_with("second") {
            total += current_num;
            current_num = 0;
        } else if part.starts_with("minute") {
            total += current_num * 60;
            current_num = 0;
        } else if part.starts_with("hour") {
            total += current_num * 3600;
            current_num = 0;
        } else if part.starts_with("day") {
            total += current_num * 86400;
            current_num = 0;
        }
    }
    total
}
