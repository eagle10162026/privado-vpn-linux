//! Async wrappers around `swanctl` and the strongSwan unit. All invocations
//! are from the daemon process (which runs as root via systemd).

use crate::daemon::proto::VpnStatus;
use tokio::process::Command;
use tracing::{info, warn};

const IPSEC_CONF: &str = "/etc/swanctl/conf.d/privado.conf";
const IPSEC_SECRETS: &str = "/etc/swanctl/conf.d/privado-secrets.conf";
const CONN_NAME: &str = "privado";

/// Live IPsec status. Reads `swanctl --list-sas` and parses the privado
/// connection block.
pub async fn live_status() -> VpnStatus {
    let out = match Command::new("swanctl").arg("--list-sas").output().await {
        Ok(o) => o,
        Err(e) => { warn!("swanctl --list-sas: {e}"); return VpnStatus::default(); }
    };
    let text = String::from_utf8_lossy(&out.stdout);
    parse_list_sas(&text)
}

fn parse_list_sas(text: &str) -> VpnStatus {
    let mut s = VpnStatus::default();
    let mut in_privado = false;
    let mut server = None::<String>;
    let mut remote_ip = None::<String>;
    let mut local_vip = None::<String>;
    let mut duration_secs = 0u64;
    let mut bytes_in = 0u64;
    let mut bytes_out = 0u64;

    for line in text.lines() {
        let l = line.trim_end();
        if l.starts_with("privado") && l.contains("ESTABLISHED") {
            in_privado = true;
            continue;
        }
        if !in_privado { continue; }
        let lt = l.trim_start();

        if lt.starts_with("local ") {
            if let Some(start) = lt.rfind('[') {
                if let Some(end) = lt[start+1..].find(']') {
                    local_vip = Some(lt[start+1..start+1+end].to_string());
                }
            }
        }
        if lt.starts_with("remote ") {
            if let Some(q1) = lt.find('\'') {
                if let Some(q2) = lt[q1+1..].find('\'') {
                    server = Some(lt[q1+1..q1+1+q2].to_string());
                }
            }
            if let Some(at) = lt.find('@') {
                let rest = lt[at+1..].trim();
                if let Some(br) = rest.find('[') { remote_ip = Some(rest[..br].trim().to_string()); }
            }
        }
        if let Some(rest) = lt.strip_prefix("established ") {
            duration_secs = parse_duration(rest);
        }
        if lt.starts_with("in ")  || lt.starts_with("in\t")  {
            bytes_in  = bytes_before_label(lt).unwrap_or(0);
        }
        if lt.starts_with("out ") || lt.starts_with("out\t") {
            bytes_out = bytes_before_label(lt).unwrap_or(0);
        }
    }

    if in_privado && server.is_some() {
        s.connected = true;
        s.server = server;
        s.remote_ip = remote_ip;
        s.local_vip = local_vip;
        s.bytes_in = bytes_in;
        s.bytes_out = bytes_out;
        s.duration_secs = duration_secs;
        s.full_tunnel = true;
    }
    s
}

/// Extract the byte count from a `swanctl --list-sas` child traffic line.
///
/// Lines look like `in  c1a2b3d4,  1234 bytes,  10 packets`. The SPI
/// (`c1a2b3d4`) contains ASCII digits, so a naive "first run of digits" parse
/// returns garbage from the SPI instead of the real count. Instead, locate the
/// comma-separated segment that contains the literal `bytes` and parse the
/// integer at the front of that segment.
fn bytes_before_label(line: &str) -> Option<u64> {
    line.split(',')
        .find(|seg| seg.contains("bytes"))
        .and_then(|seg| seg.split_whitespace().next())
        .and_then(|n| n.parse::<u64>().ok())
}

fn parse_duration(s: &str) -> u64 {
    let mut total = 0u64;
    let mut buf = 0u64;
    for c in s.chars() {
        if c.is_ascii_digit() {
            buf = buf.saturating_mul(10).saturating_add((c as u8 - b'0') as u64);
        } else {
            match c {
                's' => { total = total.saturating_add(buf); buf = 0; }
                'm' => { total = total.saturating_add(buf.saturating_mul(60)); buf = 0; }
                'h' => { total = total.saturating_add(buf.saturating_mul(3600)); buf = 0; }
                'd' => { total = total.saturating_add(buf.saturating_mul(86400)); buf = 0; }
                _ => {}
            }
        }
    }
    total
}

/// Number of `privado*` IKE_SAs currently in ESTABLISHED state.
pub async fn count_established_privado_sas() -> usize {
    let out = match Command::new("swanctl").arg("--list-sas").output().await {
        Ok(o) => o,
        Err(_) => return 0,
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.starts_with("privado") && l.contains("ESTABLISHED"))
        .count()
}

/// Ensure strongSwan is running and reload configs.
pub async fn ensure_strongswan_up() -> Result<(), String> {
    let is_active = Command::new("systemctl")
        .args(["is-active", "strongswan"])
        .output().await
        .map_err(|e| format!("systemctl is-active: {e}"))?;
    let active = String::from_utf8_lossy(&is_active.stdout).trim().to_string();
    if active != "active" {
        let r = Command::new("systemctl")
            .args(["start", "strongswan"])
            .output().await
            .map_err(|e| format!("systemctl start strongswan: {e}"))?;
        if !r.status.success() {
            return Err(format!(
                "failed to start strongswan: {}",
                String::from_utf8_lossy(&r.stderr).trim(),
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    let _ = Command::new("swanctl").arg("--load-all").output().await;
    Ok(())
}

/// Write a dynamic swanctl connection config for any server hostname.
/// This replaces the old per-country config approach.
pub async fn write_dynamic_config(
    server_host: &str,
    username: &str,
    password: &str,
    routes: &[String],
    _dns: &[String],
) -> Result<(), String> {
    let ts = if routes.is_empty() { "0.0.0.0/0".to_string() } else { routes.join(", ") };
    // strongSwan 6.x removed the connection-level `dns` option.
    // DNS is handled via resolv.conf override in routing::install_dns_override().

    // mark_out = 0x1016 means the XFRM encrypt policy ONLY matches packets
    // that already carry fwmark 0x1016. Everything else passes direct to the
    // internet without touching the VPN. This is "opt-in" tunnel mode:
    // only apps explicitly marked (Stygian, etc.) go through the VPN.
    // Chrome, IDE, LM Studio, BOBAI services all stay on direct internet.
    let conf = format!(
        r#"connections {{
  {CONN_NAME} {{
    version = 2
    remote_addrs = {server_host}
    vips = 0.0.0.0, ::
    proposals = aes256-sha256-modp2048,aes256-sha384-ecp384,default
    dpd_delay = 30s
    reauth_time = 0
    rekey_time = 0
    local {{
      auth = eap-mschapv2
      eap_id = {username}
    }}
    remote {{
      auth = pubkey
      id = vpn.privado.io
    }}
    children {{
      {CONN_NAME}-child {{
        local_ts = 0.0.0.0/0
        remote_ts = {ts}
        mark_out = 0x1016
        mark_in = 0x1016
        esp_proposals = aes256-sha256,aes256-sha384,default
        start_action = none
        dpd_action = restart
        close_action = none
        rekey_time = 0
        set_mark_out = 0x1016
      }}
    }}
  }}
}}
"#
    );

    let secrets = format!(
        r#"secrets {{
  eap-{CONN_NAME} {{
    id = {username}
    secret = "{password}"
  }}
}}
"#
    );

    // Write config files.
    tokio::fs::create_dir_all("/etc/swanctl/conf.d/").await
        .map_err(|e| format!("create conf dir: {e}"))?;
    tokio::fs::write(IPSEC_CONF, &conf).await
        .map_err(|e| format!("write {IPSEC_CONF}: {e}"))?;
    tokio::fs::write(IPSEC_SECRETS, &secrets).await
        .map_err(|e| format!("write {IPSEC_SECRETS}: {e}"))?;

    // chmod 600 on secrets file.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = tokio::fs::set_permissions(IPSEC_SECRETS, std::fs::Permissions::from_mode(0o600)).await;
    }

    // Reload swanctl with the new config.
    let load = Command::new("swanctl").arg("--load-all").output().await
        .map_err(|e| format!("swanctl --load-all: {e}"))?;
    if !load.status.success() {
        let stderr = String::from_utf8_lossy(&load.stderr);
        if !stderr.contains("already loaded") {
            return Err(format!("swanctl --load-all: {}", strip_plugin_noise(&stderr)));
        }
    }

    info!("[swanctl] config written for {server_host}");
    Ok(())
}

/// Initiate connection using the dynamic "privado" connection name.
pub async fn initiate_dynamic() -> Result<(), String> {
    let child = format!("{CONN_NAME}-child");
    let r = Command::new("swanctl")
        .args(["--initiate", "--child", &child])
        .output().await
        .map_err(|e| format!("swanctl --initiate: {e}"))?;
    if r.status.success() {
        Ok(())
    } else {
        Err(strip_plugin_noise(&String::from_utf8_lossy(&r.stderr)))
    }
}

/// Initiate with explicit ike name (legacy per-country support).
#[allow(dead_code)]
pub async fn initiate(country: &str) -> Result<(), String> {
    let ike   = format!("privado-{country}");
    let child = format!("privado-{country}-child");
    let r = Command::new("swanctl")
        .args(["--initiate", "--ike", &ike, "--child", &child])
        .output().await
        .map_err(|e| format!("swanctl --initiate: {e}"))?;
    if r.status.success() {
        Ok(())
    } else {
        Err(strip_plugin_noise(&String::from_utf8_lossy(&r.stderr)))
    }
}

/// Terminate all privado-* connections.
pub async fn terminate_all_privado() {
    // Terminate the dynamic connection.
    let _ = Command::new("swanctl")
        .args(["--terminate", "--ike", CONN_NAME])
        .output().await;
    // Also terminate any legacy per-country connections.
    for cc in ["nl", "sg", "mx"] {
        let ike = format!("privado-{cc}");
        let _ = Command::new("swanctl")
            .args(["--terminate", "--ike", &ike])
            .output().await;
    }
}

/// Remove the dynamic config files after disconnect.
pub async fn cleanup_config() {
    let _ = tokio::fs::remove_file(IPSEC_CONF).await;
    let _ = tokio::fs::remove_file(IPSEC_SECRETS).await;
    let _ = Command::new("swanctl").arg("--load-all").output().await;
}

/// Remove stale legacy per-country conf files (privado-{nl,sg,mx}.conf) left
/// over from the old per-country approach. Those files set
/// `remote_ts = 0.0.0.0/0` WITHOUT `mark_out`, so `swanctl --load-all` would
/// load a CATCH-ALL connection that — if ever initiated — captures ALL traffic
/// (LAN, the paired S22 phone, forwarded packets) into the tunnel and wrecks
/// the host network stack. The dynamic `privado.conf` (mark_out=0x1016
/// split-tunnel) is the ONLY valid config. Called at daemon startup so a stale
/// file can never silently re-arm the network-kill bug.
pub async fn purge_stale_confs() {
    for cc in ["nl", "sg", "mx"] {
        let _ = tokio::fs::remove_file(format!("/etc/swanctl/conf.d/privado-{cc}.conf")).await;
    }
    let _ = Command::new("swanctl").arg("--load-all").output().await;
}

fn strip_plugin_noise(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|l| !l.starts_with("plugin '") && !l.contains("failed to load"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_before_label_ignores_hex_spi_digits() {
        // The SPI `c1a2b3d4` contains digits 1,2,3,4 — the parser must NOT
        // return those; it must return the count immediately before `bytes`.
        assert_eq!(bytes_before_label("in  c1a2b3d4,  1234 bytes,  10 packets"), Some(1234));
        assert_eq!(bytes_before_label("out e5f60718,  98765 bytes,  42 packets"), Some(98765));
    }

    #[test]
    fn parse_list_sas_reads_real_byte_counts() {
        // A representative `swanctl --list-sas` block with hex SPIs in the
        // child traffic lines. Byte counts must come from before `bytes`, not
        // from digits embedded in the SPI.
        let sample = "\
privado: #1, ESTABLISHED, IKEv2, abc123def456_i* 0011223344556677_r*
  local  'user@example' @ 10.0.0.2[4500]
  remote 'vpn.privado.io' @ 185.107.56.10[4500]
  established 123s ago, rekeying in 0s
  privado-child: #1, reqid 1, INSTALLED, TUNNEL, ESP:AES_GCM_16-256
    installed 123s ago
    in  c1a2b3d4,  524288 bytes,  900 packets
    out 1b2c3d4e,  131072 bytes,  450 packets
    local  0.0.0.0/0
    remote 0.0.0.0/0";
        let status = parse_list_sas(sample);
        assert!(status.connected);
        assert_eq!(status.server.as_deref(), Some("vpn.privado.io"));
        assert_eq!(status.bytes_in, 524288);
        assert_eq!(status.bytes_out, 131072);
        assert_eq!(status.duration_secs, 123);
    }
}
