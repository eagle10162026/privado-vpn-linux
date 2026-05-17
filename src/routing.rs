//! Policy routing engine for the PrivadoVPN daemon.
//!
//! Three layers:
//!
//! 1. **Kill-switch** — iptables chain PRIVADO_KILLSWITCH that blocks
//!    non-VPN traffic to split-tunnel domains when the VPN is down.
//!
//! 2. **Policy routing** — `ip rule` / fwmark to force VPN traffic
//!    through the WiFi interface while leaving LAN traffic untouched.
//!    Uses fwmark 0x1234 and routing table 1234.
//!
//! 3. **DNS override** — writes a resolv.conf drop-in for the VPN's
//!    DNS servers when connected, restores the original on disconnect.

use std::net::ToSocketAddrs;
use std::process::Command;
use tracing::{info, warn};

const IPTABLES_CHAIN: &str = "PRIVADO_KILLSWITCH";

/// fwmark value for VPN-bound packets. Chosen to avoid collisions with common fwmarks.
const VPN_FWMARK: u32 = 0x1234;

/// Routing table number for VPN policy routes.
const VPN_TABLE: u32 = 1234;

// ─── DNS resolution helpers ────────────────────────────────────────────────

/// Resolve a list of domain names to their IP addresses (v4 and v6).
pub fn resolve_domain_ips(domains: &[String]) -> Vec<String> {
    let mut ips = Vec::new();
    for domain in domains {
        let lookup = if domain.contains(':') {
            domain.to_string()
        } else {
            format!("{domain}:443")
        };
        match lookup.to_socket_addrs() {
            Ok(addrs) => {
                for addr in addrs {
                    let ip = addr.ip().to_string();
                    if !ips.contains(&ip) {
                        ips.push(ip);
                    }
                }
            }
            Err(e) => {
                warn!("[routing] DNS resolve failed for {domain}: {e}");
            }
        }
    }
    info!("[routing] resolved {} IPs from {} domains", ips.len(), domains.len());
    ips
}

/// Convert a bare IP to a CIDR notation (/32 for IPv4, /128 for IPv6).
pub fn ip_to_cidr(ip: &str) -> String {
    if ip.contains(':') {
        format!("{ip}/128")
    } else {
        format!("{ip}/32")
    }
}

/// Resolve domains to CIDR routes for split-tunnel use.
/// Exported for CLI use (e.g. `privado-vpn routes`).
#[allow(dead_code)]
pub fn generate_split_routes(domains: &[String]) -> Vec<String> {
    resolve_domain_ips(domains)
        .into_iter()
        .map(|ip| ip_to_cidr(&ip))
        .collect()
}

// ─── Kill-switch (iptables) ────────────────────────────────────────────────

/// Install iptables kill-switch rules that block traffic to the given domains
/// unless it's going through the IPsec tunnel (ipsec policy match) or loopback.
pub fn install_killswitch(domains: &[String]) -> Result<(), String> {
    let ips = resolve_domain_ips(domains);
    if ips.is_empty() {
        info!("[killswitch] no domains to protect — skipping");
        return Ok(());
    }

    // Create chain (ignore error if it already exists).
    run_cmd("iptables", &["-N", IPTABLES_CHAIN]).ok();
    run_cmd("iptables", &["-F", IPTABLES_CHAIN])?;

    for ip in &ips {
        // Always allow loopback.
        run_cmd("iptables", &["-A", IPTABLES_CHAIN, "-d", ip, "-o", "lo", "-j", "ACCEPT"])?;
        // Allow if traffic is going through an IPsec SA (VPN is up).
        run_cmd("iptables", &[
            "-A", IPTABLES_CHAIN, "-d", ip,
            "-m", "policy", "--dir", "out", "--pol", "ipsec",
            "-j", "ACCEPT",
        ])?;
        // Block everything else to these IPs.
        run_cmd("iptables", &[
            "-A", IPTABLES_CHAIN, "-d", ip,
            "-j", "REJECT", "--reject-with", "icmp-net-unreachable",
        ])?;
    }

    // Jump from OUTPUT chain if not already present.
    let check = Command::new("iptables")
        .args(["-C", "OUTPUT", "-j", IPTABLES_CHAIN])
        .output()
        .map_err(|e| format!("iptables check failed: {e}"))?;

    if !check.status.success() {
        run_cmd("iptables", &["-A", "OUTPUT", "-j", IPTABLES_CHAIN])?;
    }

    info!("[killswitch] installed — {} IPs blocked without VPN", ips.len());
    Ok(())
}

/// Remove the kill-switch chain and all its rules.
pub fn remove_killswitch() -> Result<(), String> {
    let _ = run_cmd("iptables", &["-D", "OUTPUT", "-j", IPTABLES_CHAIN]);
    let _ = run_cmd("iptables", &["-F", IPTABLES_CHAIN]);
    let _ = run_cmd("iptables", &["-X", IPTABLES_CHAIN]);
    info!("[killswitch] removed");
    Ok(())
}

// ─── Policy routing (ip rule + ip route) ───────────────────────────────────

/// Detect the primary WiFi interface (wlp* or wlan*).
fn detect_wifi_interface() -> Option<String> {
    let output = Command::new("ip")
        .args(["-o", "link", "show"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        // Format: "2: wlp6s0: <BROADCAST,...>"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let iface = parts[1].trim_end_matches(':');
            if iface.starts_with("wl") {
                return Some(iface.to_string());
            }
        }
    }
    None
}

/// Get the default gateway for a given interface from the main routing table.
fn get_gateway_for_iface(iface: &str) -> Option<String> {
    let output = Command::new("ip")
        .args(["route", "show", "default", "dev", iface])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    // "default via 192.168.0.1 dev wlp6s0 ..."
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 && parts[0] == "default" && parts[1] == "via" {
            return Some(parts[2].to_string());
        }
    }
    None
}

/// Install policy routing rules that force VPN (IPsec) traffic through WiFi
/// while leaving LAN traffic untouched.
///
/// Mechanism:
///   1. Mark outbound packets destined for VPN endpoints with fwmark 0x1234
///   2. Add `ip rule` sending marked packets to table 1234
///   3. Table 1016 has a default route via the WiFi gateway
///
/// This ensures the VPN tunnel always goes out over WiFi even if there's
/// a wired interface or other default route.
pub fn install_policy_routing(vpn_remote_ips: &[String]) -> Result<(), String> {
    let wifi = detect_wifi_interface()
        .ok_or_else(|| "no WiFi interface found (wl*)".to_string())?;
    let gw = get_gateway_for_iface(&wifi)
        .ok_or_else(|| format!("no default gateway on {wifi}"))?;

    info!("[policy-route] WiFi={wifi}, gateway={gw}, table={VPN_TABLE}, fwmark=0x{VPN_FWMARK:x}");

    // Clean up any stale rules/routes from a previous run.
    let _ = run_cmd("ip", &["rule", "del", "fwmark", &format!("0x{VPN_FWMARK:x}"), "table", &VPN_TABLE.to_string()]);
    let _ = run_cmd("ip", &["route", "flush", "table", &VPN_TABLE.to_string()]);

    // Add the policy route table: default via WiFi gateway.
    run_cmd("ip", &[
        "route", "add", "default",
        "via", &gw, "dev", &wifi,
        "table", &VPN_TABLE.to_string(),
    ])?;

    // Mark packets to VPN endpoints.
    for ip in vpn_remote_ips {
        let cidr = ip_to_cidr(ip);
        // Remove stale rule if present, then add fresh.
        let _ = run_cmd("iptables", &[
            "-t", "mangle", "-D", "OUTPUT",
            "-d", &cidr,
            "-j", "MARK", "--set-mark", &format!("0x{VPN_FWMARK:x}"),
        ]);
        run_cmd("iptables", &[
            "-t", "mangle", "-A", "OUTPUT",
            "-d", &cidr,
            "-j", "MARK", "--set-mark", &format!("0x{VPN_FWMARK:x}"),
        ])?;
    }

    // ip rule: fwmark → table 1234.
    run_cmd("ip", &[
        "rule", "add", "fwmark", &format!("0x{VPN_FWMARK:x}"),
        "table", &VPN_TABLE.to_string(),
        "priority", "100",
    ])?;

    info!("[policy-route] installed for {} VPN endpoints", vpn_remote_ips.len());
    Ok(())
}

/// Remove all policy routing rules and the VPN routing table.
pub fn remove_policy_routing(vpn_remote_ips: &[String]) -> Result<(), String> {
    // Remove ip rule.
    let _ = run_cmd("ip", &["rule", "del", "fwmark", &format!("0x{VPN_FWMARK:x}"), "table", &VPN_TABLE.to_string()]);

    // Remove mangle marks.
    for ip in vpn_remote_ips {
        let cidr = ip_to_cidr(ip);
        let _ = run_cmd("iptables", &[
            "-t", "mangle", "-D", "OUTPUT",
            "-d", &cidr,
            "-j", "MARK", "--set-mark", &format!("0x{VPN_FWMARK:x}"),
        ]);
    }

    // Flush the policy table.
    let _ = run_cmd("ip", &["route", "flush", "table", &VPN_TABLE.to_string()]);

    info!("[policy-route] removed");
    Ok(())
}

// ─── DNS override ──────────────────────────────────────────────────────────

const RESOLVCONF_BACKUP: &str = "/run/privado-vpn-resolv.bak";
const RESOLVCONF: &str = "/etc/resolv.conf";

/// Override /etc/resolv.conf with VPN DNS as primary but KEEP existing
/// system DNS as fallback. This ensures BOBAI services (free-ai-swarm,
/// bob-runtime, etc.) can still resolve external API domains even if the
/// VPN tunnel hiccups or Privado's DNS doesn't resolve non-web domains.
pub fn install_dns_override(servers: &[String]) -> Result<(), String> {
    if servers.is_empty() {
        return Ok(());
    }

    // Back up current resolv.conf (only if backup doesn't already exist from
    // a previous connect-without-disconnect).
    if !std::path::Path::new(RESOLVCONF_BACKUP).exists() {
        if let Ok(content) = std::fs::read_to_string(RESOLVCONF) {
            std::fs::write(RESOLVCONF_BACKUP, &content)
                .map_err(|e| format!("backup resolv.conf: {e}"))?;
        }
    }

    // Extract existing nameservers from the backup to preserve as fallback.
    let existing_nameservers: Vec<String> = std::fs::read_to_string(RESOLVCONF_BACKUP)
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("nameserver "))
        .map(|l| l.trim_start_matches("nameserver ").trim().to_string())
        .filter(|ns| !servers.contains(ns))
        .collect();

    let mut content = String::from("# Generated by privado-vpn daemon — will be restored on disconnect\n");
    // VPN DNS servers first (primary for tunnel-routed traffic).
    for s in servers {
        content.push_str(&format!("nameserver {s}\n"));
    }
    // System DNS as fallback (keeps BOBAI services working for external APIs).
    for ns in &existing_nameservers {
        content.push_str(&format!("nameserver {ns}\n"));
    }
    std::fs::write(RESOLVCONF, &content)
        .map_err(|e| format!("write resolv.conf: {e}"))?;

    info!("[dns] overrode resolv.conf: {} VPN + {} fallback servers", servers.len(), existing_nameservers.len());
    Ok(())
}

/// Restore the original /etc/resolv.conf from the backup.
pub fn restore_dns() -> Result<(), String> {
    if std::path::Path::new(RESOLVCONF_BACKUP).exists() {
        std::fs::copy(RESOLVCONF_BACKUP, RESOLVCONF)
            .map_err(|e| format!("restore resolv.conf: {e}"))?;
        let _ = std::fs::remove_file(RESOLVCONF_BACKUP);
        info!("[dns] restored original resolv.conf");
    }
    Ok(())
}

// ─── Full lifecycle ────────────────────────────────────────────────────────

/// Called by the daemon after a successful VPN connection. Installs:
/// - Policy routing (marked traffic → VPN table)
/// - DNS override (VPN DNS primary, system DNS fallback)
/// - VPN cgroup for opt-in app routing (Stygian, etc.)
/// - Kill switch (only for split-tunnel domain protection)
///
/// DEFAULT: Nothing goes through the VPN unless explicitly marked.
/// Chrome, IDE, LM Studio, BOBAI services all stay on direct internet.
/// Only apps launched inside the VPN cgroup get tunneled.
pub fn on_connect(
    vpn_remote_ips: &[String],
    dns_servers: &[String],
    kill_switch_enabled: bool,
    split_domains: &[String],
) {
    if let Err(e) = install_policy_routing(vpn_remote_ips) {
        warn!("[routing] policy routing failed: {e}");
    }
    if let Err(e) = install_dns_override(dns_servers) {
        warn!("[routing] DNS override failed: {e}");
    }
    if let Err(e) = install_vpn_cgroup() {
        warn!("[routing] VPN cgroup setup failed: {e}");
    }
    if kill_switch_enabled && !split_domains.is_empty() {
        if let Err(e) = install_killswitch(split_domains) {
            warn!("[routing] killswitch install failed: {e}");
        }
    }
}

// ─── VPN Cgroup (opt-in app routing) ──────────────────────────────────────
// Only apps placed in this cgroup get their traffic routed through the VPN.
// Stygian AI browser is the primary user. Launch apps through the VPN with:
//   cgexec -g net_cls:privado_vpn <command>
// or write the PID to /sys/fs/cgroup/net_cls/privado_vpn/cgroup.procs

const VPN_CGROUP_PATH: &str = "/sys/fs/cgroup/net_cls/privado_vpn";
const VPN_CGROUP_CLASSID: u32 = 0x00123400;

/// Create the net_cls cgroup and iptables rule that marks traffic from
/// processes in that cgroup with fwmark 0x1234. Only this marked traffic
/// hits the strongSwan XFRM encrypt policy (mark_out = 0x1234).
pub fn install_vpn_cgroup() -> Result<(), String> {
    // Create the cgroup directory.
    let _ = std::fs::create_dir_all(VPN_CGROUP_PATH);

    // Write the classid that identifies packets from this cgroup.
    let classid_path = format!("{VPN_CGROUP_PATH}/net_cls.classid");
    std::fs::write(&classid_path, format!("{VPN_CGROUP_CLASSID}\n"))
        .map_err(|e| format!("write cgroup classid: {e}"))?;

    // iptables: mark packets from the VPN cgroup with fwmark 0x1234.
    // This mark is what strongSwan's XFRM policy matches against.
    let _ = run_cmd("iptables", &[
        "-t", "mangle", "-D", "OUTPUT",
        "-m", "cgroup", "--cgroup", &format!("0x{VPN_CGROUP_CLASSID:08x}"),
        "-j", "MARK", "--set-mark", &format!("0x{VPN_FWMARK:x}"),
    ]);
    run_cmd("iptables", &[
        "-t", "mangle", "-A", "OUTPUT",
        "-m", "cgroup", "--cgroup", &format!("0x{VPN_CGROUP_CLASSID:08x}"),
        "-j", "MARK", "--set-mark", &format!("0x{VPN_FWMARK:x}"),
    ])?;

    info!("[routing] VPN cgroup installed at {VPN_CGROUP_PATH} — only apps in this cgroup use the tunnel");
    Ok(())
}

/// Remove the VPN cgroup iptables rule on disconnect.
pub fn remove_vpn_cgroup() {
    let _ = run_cmd("iptables", &[
        "-t", "mangle", "-D", "OUTPUT",
        "-m", "cgroup", "--cgroup", &format!("0x{VPN_CGROUP_CLASSID:08x}"),
        "-j", "MARK", "--set-mark", &format!("0x{VPN_FWMARK:x}"),
    ]);
    info!("[routing] VPN cgroup iptables rule removed");
}

// The old BOBAI bypass is no longer needed — the mark_out approach means
// ONLY explicitly marked traffic (from the VPN cgroup) enters the tunnel.
// Everything else (Chrome, IDE, LM Studio, BOBAI services) goes direct
// without any bypass rules needed.

/// Called by the daemon before or after disconnecting the VPN. Tears down
/// policy routing, DNS override, VPN cgroup, and the kill-switch.
pub fn on_disconnect(vpn_remote_ips: &[String]) {
    if let Err(e) = remove_policy_routing(vpn_remote_ips) {
        warn!("[routing] policy routing removal failed: {e}");
    }
    if let Err(e) = restore_dns() {
        warn!("[routing] DNS restore failed: {e}");
    }
    remove_vpn_cgroup();
    if let Err(e) = remove_killswitch() {
        warn!("[routing] killswitch removal failed: {e}");
    }
}

// ─── Utility ───────────────────────────────────────────────────────────────

fn run_cmd(program: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("{program} failed: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{program} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}
