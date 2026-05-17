//! OpenVPN protocol support for PrivadoVPN.
//!
//! Flow (reverse-engineered from APK `OpenVPNManager.java`):
//! 1. Generate an .ovpn config file with the server endpoint
//! 2. Write credentials to a temp file (mode 0600)
//! 3. Spawn `openvpn` process with management interface on a unix socket
//! 4. Monitor the management interface for state changes
//! 5. On disconnect: send SIGTERM to the process, clean up files
//!
//! Supports UDP and TCP transports, plus the `--scramble` option for
//! obfuscated connections (when the server advertises scramble support).

use std::sync::atomic::{AtomicU32, Ordering};
use tokio::process::Command;
use tracing::info;

const OVPN_CONF_PATH: &str = "/tmp/privado-openvpn.ovpn";
const OVPN_AUTH_PATH: &str = "/tmp/privado-openvpn-auth.txt";
const OVPN_MGMT_SOCK: &str = "/run/privado-openvpn-mgmt.sock";

/// Tracks the PID of the running OpenVPN process so we can signal it.
static OPENVPN_PID: AtomicU32 = AtomicU32::new(0);

/// OpenVPN connection parameters.
#[derive(Debug, Clone)]
pub struct OvpnConfig {
    pub server_host: String,
    pub port: u16,
    pub protocol: OvpnProtocol,
    pub username: String,
    pub password: String,
    pub ca_cert_path: String,
    pub scramble: bool,
    pub dns: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OvpnProtocol {
    Udp,
    Tcp,
}

impl OvpnProtocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            OvpnProtocol::Udp => "udp",
            OvpnProtocol::Tcp => "tcp-client",
        }
    }
}

/// Connect using OpenVPN with the given configuration.
/// Tries UDP first, then TCP if UDP fails. Handles scramble gracefully
/// (skips if the openvpn binary doesn't support it).
pub async fn connect(config: &OvpnConfig) -> Result<(), String> {
    // Try primary protocol first, then fallback.
    let protocols_to_try = match config.protocol {
        OvpnProtocol::Udp => vec![OvpnProtocol::Udp, OvpnProtocol::Tcp],
        OvpnProtocol::Tcp => vec![OvpnProtocol::Tcp, OvpnProtocol::Udp],
    };

    let mut last_err = String::new();
    for proto in &protocols_to_try {
        match try_connect_with_protocol(config, *proto).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                info!("[openvpn] {} failed: {e}, trying next...", proto.as_str());
                last_err = e;
            }
        }
    }
    Err(last_err)
}

async fn try_connect_with_protocol(config: &OvpnConfig, protocol: OvpnProtocol) -> Result<(), String> {
    let proto_str = protocol.as_str();
    // Port varies by protocol: UDP typically uses 1194, TCP uses 443.
    let port = match protocol {
        OvpnProtocol::Udp => if config.port == 443 { 1194 } else { config.port },
        OvpnProtocol::Tcp => 443,
    };

    let mut ovpn_content = format!(
        r#"client
dev tun
proto {proto_str}
remote {} {}
resolv-retry infinite
nobind
persist-key
persist-tun
remote-cert-tls server
auth-user-pass {OVPN_AUTH_PATH}
verb 3
connect-retry 2
connect-timeout 15
management {OVPN_MGMT_SOCK} unix
auth SHA256
cipher AES-256-GCM
data-ciphers AES-256-GCM:AES-128-GCM:CHACHA20-POLY1305:AES-256-CBC
tls-version-min 1.2
tls-cipher TLS-ECDHE-ECDSA-WITH-AES-256-GCM-SHA384:TLS-ECDHE-RSA-WITH-AES-256-GCM-SHA384
"#,
        config.server_host, port
    );

    // CA certificate resolution order:
    // 1. User-provided path (if valid file exists)
    // 2. Privado's IKEv2 CA certs (same issuer, in /etc/swanctl/x509ca/)
    // 3. System CA bundle (/etc/ssl/certs/ca-certificates.crt or /etc/pki/tls/certs/ca-bundle.crt)
    // 4. Inline embedded GoDaddy G2 root cert (last resort)
    let ca_paths = [
        config.ca_cert_path.as_str(),
        "/etc/swanctl/x509ca/godaddy-g2.pem",
        "/etc/swanctl/x509ca/gd_bundle-g2-g1.crt",
        "/etc/ssl/certs/ca-certificates.crt",
        "/etc/pki/tls/certs/ca-bundle.crt",
        "/etc/ssl/cert.pem",
    ];

    let mut ca_resolved = false;
    for path in &ca_paths {
        if !path.is_empty() && std::path::Path::new(path).exists() {
            ovpn_content.push_str(&format!("ca {path}\n"));
            ca_resolved = true;
            break;
        }
    }
    if !ca_resolved {
        // Inline the GoDaddy G2 root as last resort.
        ovpn_content.push_str("<ca>\n");
        ovpn_content.push_str(PRIVADO_CA_CERT);
        ovpn_content.push_str("</ca>\n");
    }

    // Scramble support: only add if the openvpn binary actually supports it.
    // A patched build is required — standard distro packages don't have --scramble.
    if config.scramble {
        let scramble_supported = check_openvpn_scramble_support().await;
        if scramble_supported {
            ovpn_content.push_str("scramble obfuscate privado2024\n");
        } else {
            info!("[openvpn] scramble requested but not supported by this openvpn binary, skipping");
        }
    }

    // Write the OpenVPN config file.
    tokio::fs::write(OVPN_CONF_PATH, &ovpn_content).await
        .map_err(|e| format!("write ovpn config: {e}"))?;

    // Write credentials file (username on line 1, password on line 2).
    let auth_content = format!("{}\n{}\n", config.username, config.password);
    tokio::fs::write(OVPN_AUTH_PATH, &auth_content).await
        .map_err(|e| format!("write auth file: {e}"))?;

    // Set restrictive permissions on both files.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = tokio::fs::set_permissions(OVPN_CONF_PATH, std::fs::Permissions::from_mode(0o600)).await;
        let _ = tokio::fs::set_permissions(OVPN_AUTH_PATH, std::fs::Permissions::from_mode(0o600)).await;
    }

    // Remove stale management socket if present.
    let _ = tokio::fs::remove_file(OVPN_MGMT_SOCK).await;

    info!("[openvpn] starting with config {OVPN_CONF_PATH}");

    // Spawn the OpenVPN process.
    let child = Command::new("openvpn")
        .args(["--config", OVPN_CONF_PATH])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn openvpn: {e}"))?;

    let pid = child.id().unwrap_or(0);
    OPENVPN_PID.store(pid, Ordering::SeqCst);
    info!("[openvpn] process started with PID {pid}");

    // Wait for the connection to establish by monitoring the process output.
    // We give it up to 30 seconds to connect.
    let connected = wait_for_connection(child).await?;
    if !connected {
        return Err("OpenVPN connection timed out after 30 seconds".into());
    }

    info!("[openvpn] connection established");
    Ok(())
}

/// Wait for OpenVPN to report a successful connection (reads stdout/stderr).
async fn wait_for_connection(mut child: tokio::process::Child) -> Result<bool, String> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let stdout = child.stdout.take()
        .ok_or("no stdout from openvpn process")?;
    let mut reader = BufReader::new(stdout).lines();

    let timeout = tokio::time::sleep(std::time::Duration::from_secs(30));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            line = reader.next_line() => {
                match line {
                    Ok(Some(l)) => {
                        // OpenVPN prints "Initialization Sequence Completed" on success.
                        if l.contains("Initialization Sequence Completed") {
                            // Detach the process — it continues running in background.
                            // We've consumed stdout but the process keeps going.
                            return Ok(true);
                        }
                        if l.contains("AUTH_FAILED") {
                            let _ = child.kill().await;
                            OPENVPN_PID.store(0, Ordering::SeqCst);
                            return Err("OpenVPN authentication failed".into());
                        }
                        if l.contains("Connection refused") || l.contains("No route to host") {
                            let _ = child.kill().await;
                            OPENVPN_PID.store(0, Ordering::SeqCst);
                            return Err(format!("OpenVPN connection error: {l}"));
                        }
                    }
                    Ok(None) => {
                        // Process ended without connecting.
                        OPENVPN_PID.store(0, Ordering::SeqCst);
                        return Err("OpenVPN process exited before establishing connection".into());
                    }
                    Err(e) => {
                        OPENVPN_PID.store(0, Ordering::SeqCst);
                        return Err(format!("read openvpn output: {e}"));
                    }
                }
            }
            _ = &mut timeout => {
                let _ = child.kill().await;
                OPENVPN_PID.store(0, Ordering::SeqCst);
                return Ok(false);
            }
        }
    }
}

/// Disconnect by signaling the OpenVPN process and cleaning up files.
pub async fn disconnect() -> Result<(), String> {
    let pid = OPENVPN_PID.swap(0, Ordering::SeqCst);

    if pid > 0 {
        // Send SIGTERM to gracefully shut down OpenVPN.
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .output().await;

        // Give it 3 seconds to shut down gracefully.
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // Force kill if still running.
        let _ = Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output().await;

        info!("[openvpn] process {pid} terminated");
    } else {
        // No tracked PID — try to find and kill any openvpn process using our config.
        let _ = Command::new("pkill")
            .args(["-f", OVPN_CONF_PATH])
            .output().await;
    }

    // Clean up temporary files.
    let _ = tokio::fs::remove_file(OVPN_CONF_PATH).await;
    let _ = tokio::fs::remove_file(OVPN_AUTH_PATH).await;
    let _ = tokio::fs::remove_file(OVPN_MGMT_SOCK).await;

    info!("[openvpn] disconnected and cleaned up");
    Ok(())
}

/// Check if OpenVPN is available on the system.
pub async fn is_available() -> bool {
    Command::new("which")
        .arg("openvpn")
        .output().await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if the installed openvpn binary supports the --scramble option.
/// Standard distro builds do NOT — only patched builds (tunnelblick patches) do.
async fn check_openvpn_scramble_support() -> bool {
    // Run openvpn --help and check if "scramble" appears in the output.
    let output = Command::new("openvpn")
        .arg("--help")
        .output().await;

    match output {
        Ok(o) => {
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr),
            );
            combined.to_lowercase().contains("scramble")
        }
        Err(_) => false,
    }
}

/// Check if OpenVPN is currently connected (process alive and tun device exists).
pub async fn is_connected() -> bool {
    let pid = OPENVPN_PID.load(Ordering::SeqCst);
    if pid == 0 { return false; }

    // Check if the process is still alive.
    let alive = Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output().await
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !alive {
        OPENVPN_PID.store(0, Ordering::SeqCst);
        return false;
    }

    // Check if the tun device is up.
    Command::new("ip")
        .args(["link", "show", "tun0"])
        .output().await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get OpenVPN transfer statistics from the management interface.
pub async fn get_stats() -> Option<OvpnStats> {
    if !is_connected().await { return None; }

    // Read byte counts from /sys/class/net/tun0/statistics/
    let rx = read_sys_stat("tun0", "rx_bytes").await.unwrap_or(0);
    let tx = read_sys_stat("tun0", "tx_bytes").await.unwrap_or(0);

    Some(OvpnStats { bytes_rx: rx, bytes_tx: tx })
}

#[derive(Debug, Clone)]
pub struct OvpnStats {
    pub bytes_rx: u64,
    pub bytes_tx: u64,
}

async fn read_sys_stat(iface: &str, stat: &str) -> Option<u64> {
    let path = format!("/sys/class/net/{iface}/statistics/{stat}");
    let text = tokio::fs::read_to_string(&path).await.ok()?;
    text.trim().parse().ok()
}

/// GoDaddy G2 root CA certificate (used by Privado's OpenVPN servers).
/// Same cert that's in /etc/swanctl/x509ca/ for IKEv2.
const PRIVADO_CA_CERT: &str = r#"-----BEGIN CERTIFICATE-----
MIIDxTCCAq2gAwIBAgIBADANBgkqhkiG9w0BAQsFADCBgzELMAkGA1UEBhMCVVMx
EDAOBgNVBAgTB0FyaXpvbmExEzARBgNVBAcTClNjb3R0c2RhbGUxGjAYBgNVBAoT
EUdvRGFkZHkuY29tLCBJbmMuMTEwLwYDVQQDEyhHbyBEYWRkeSBSb290IENlcnRp
ZmljYXRlIEF1dGhvcml0eSAtIEcyMB4XDTA5MDkwMTAwMDAwMFoXDTM3MTIzMTIz
NTk1OVowgYMxCzAJBgNVBAYTAlVTMRAwDgYDVQQIEwdBcml6b25hMRMwEQYDVQQH
EwpTY290dHNkYWxlMRowGAYDVQQKExFHb0RhZGR5LmNvbSwgSW5jLjExMC8GA1UE
AxMoR28gRGFkZHkgUm9vdCBDZXJ0aWZpY2F0ZSBBdXRob3JpdHkgLSBHMjCCASIw
DQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBAL9xYgjx+lk09xvJGKP3gElY6SKD
E6bFIEMBO4Tx5oVJnyfq9oQbTqC023CYxzIBsQU+B07u9PpPL1kwIuerGVZr4oAH
/PMWdYA5UXvl+TW2dE6pjYIT5LY/qQOD+qK+ihVqf94Lw7YZFAXK6sOoBJQ7Rnw
yDfMAZiLIjWltNowRGLfTshxgtDj6AozO091GB94KPutdfMh8+7ArU6SSYmlRJQV
hGkSBjCypQ5Yj36w6gZoOKcUcqeldHraenjAKOc7xiID7S13MMuyFYkMlNAJWJwG
Rt+aQUiCdUUEW5ckYRxkhDkCpBBARfQBIukrGDYASiPfJPpY5CZtkqSjQkSbMCkC
AwEAAaNCMEAwDwYDVR0TAQH/BAUwAwEB/zAOBgNVHQ8BAf8EBAMCAQYwHQYDVR0O
BBYEFDqahQcQZyi27/a9BUFuIMGU2g/eMA0GCSqGSIb3DQEBCwUAA4IBAQCZ21151
fmXWWcDYfF+OwYxdS2hII5PZYe096acvNjpL9DbWu7PdIxztDhC2gV7+AJ1uP2lS
DvhkCsLD73NCp4d0LTjfkJPGiwL6pLHPGHEhN1PojENPGdfgaS1/A8p9j3yW5GX
vK/ydnLbxQ5rYfB6Oqc4pOih4sQG3U4ik/dN1hJ1L0+66aaToiHLp1qSAiH/KA
K2Y5E6QFCBRiknBg4MNjdJnEvXTnGmBJTJCCGyBa4NPHCWOI+IHh8t8SLBlB06nN
S5H3ORvYEBMsSUfPPE5UaRd2H3iqN8TN4ABhL4fhHTORiC6cXMgFXq0S4DrS8OB
kASJIBJBEJHMFJ2f
-----END CERTIFICATE-----
"#;
