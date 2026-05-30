//! Routing-rule engine for the PrivadoVPN daemon.
//!
//! Maps the user's `RoutingRule` list (process / domain / ip-cidr / port) onto
//! the existing VPN fwmark (`0x1016` = "into the tunnel", matched by the
//! strongSwan XFRM `mark_out` policy). Everything is rendered into a SINGLE
//! dedicated nftables table `inet privado_routing` so each re-apply is an
//! atomic flush+rebuild (the previous per-rule `iptables -D/-A` dance was racy
//! and leaked rules — see R5).
//!
//! Match-type mapping (design section D):
//!
//! - process (uid:N): `meta skuid N`.
//! - process (app name): `socket cgroupv2 level 1 "privado_vpn.slice"`; the daemon ensures that cgroup-v2 slice exists. A v1 `net_cls` classid fallback is used ONLY if /sys/fs/cgroup/net_cls exists (R1).
//! - domain: resolve to /32 or /128, add to the vpn/direct set, and mirror the domain into split_domains so the connect-time remote_ts selector permits the dst.
//! - ip_cidr: add the CIDR to the vpn/direct set.
//! - port / port_range: `<proto> dport <p|lo-hi>` (full-tunnel required for action=vpn since selectors are IP-based).
//!
//! `action=Vpn`    → `meta mark set 0x1016`
//! `action=Direct` → `meta mark set 0x0` + `return` (short-circuit; wins).
//!
//! cgroup-v2 launch: `systemd-run --slice=privado_vpn.slice <cmd>` or write a
//! PID into /sys/fs/cgroup/privado_vpn.slice/cgroup.procs.

use std::process::Command;
use tracing::{info, warn};

use crate::config::{Config, RoutingRule, RuleAction, RuleMatchType};

/// The single nftables table that owns every routing-rule artifact.
const NFT_TABLE: &str = "privado_routing";

/// fwmark value that the strongSwan XFRM encrypt policy matches.
const VPN_FWMARK: &str = "0x1016";

/// cgroup-v2 slice used for app-name process rules.
const CGROUP_V2_SLICE: &str = "privado_vpn.slice";
const CGROUP_V2_SLICE_PATH: &str = "/sys/fs/cgroup/privado_vpn.slice";

/// cgroup-v1 net_cls dir (legacy fallback only).
const CGROUP_V1_NETCLS: &str = "/sys/fs/cgroup/net_cls";
const CGROUP_V1_VPN_PATH: &str = "/sys/fs/cgroup/net_cls/privado_vpn";
const CGROUP_V1_CLASSID: &str = "0x00101600";

/// Whether the host exposes a usable cgroup-v1 net_cls controller.
fn has_cgroup_v1_netcls() -> bool {
    std::path::Path::new(CGROUP_V1_NETCLS).is_dir()
}

/// Ensure the cgroup-v2 slice directory exists so app PIDs can be moved in.
/// systemd normally creates the slice on first `systemd-run --slice=`, but we
/// create the directory up front so writing cgroup.procs works immediately.
fn ensure_cgroup_v2_slice() {
    if let Err(e) = std::fs::create_dir_all(CGROUP_V2_SLICE_PATH) {
        // Not fatal: nft `socket cgroupv2` matching also works once systemd
        // creates the slice on first launch.
        warn!("[routing-rules] could not pre-create {CGROUP_V2_SLICE_PATH}: {e}");
    }
}

/// Ensure the legacy cgroup-v1 net_cls group + classid exist (fallback path).
fn ensure_cgroup_v1() {
    let _ = std::fs::create_dir_all(CGROUP_V1_VPN_PATH);
    let classid_path = format!("{CGROUP_V1_VPN_PATH}/net_cls.classid");
    if let Err(e) = std::fs::write(&classid_path, format!("{CGROUP_V1_CLASSID}\n")) {
        warn!("[routing-rules] write v1 net_cls.classid failed: {e}");
    }
}

/// Resolve a domain to its A/AAAA addresses (reuses the routing resolver).
fn resolve_domain(domain: &str) -> Vec<String> {
    crate::routing::resolve_domain_ips(std::slice::from_ref(&domain.to_string()))
}

/// Returns true if a string looks like an IPv6 literal / CIDR.
fn is_v6(s: &str) -> bool {
    s.contains(':')
}

/// Normalise a bare IP to a CIDR, leaving an existing `/n` untouched.
fn to_cidr(ip: &str) -> String {
    if ip.contains('/') {
        ip.to_string()
    } else if is_v6(ip) {
        format!("{ip}/128")
    } else {
        format!("{ip}/32")
    }
}

/// Effective rule list = user rules + any synthesized managed rules (R2).
fn effective_rules(cfg: &Config) -> Vec<RoutingRule> {
    let mut rules = cfg.routing_rules.clone();
    rules.extend(synthesize_managed_rules(cfg));
    // Evaluate in ascending priority. Direct rules emit `return` so ordering
    // matters: lower priority is rendered first.
    rules.sort_by_key(|r| r.priority);
    rules
}

/// R2: synthesize managed Process rules from the route_llm_* toggles so the UI
/// toggles actually drive routing through the same engine. The managed rules
/// are NOT persisted to config — they are derived each apply.
fn synthesize_managed_rules(cfg: &Config) -> Vec<RoutingRule> {
    let mut out = Vec::new();
    if cfg.route_llm_browser {
        out.push(RoutingRule {
            id: "managed-llm-browser".to_string(),
            enabled: true,
            name: "managed:llm_browser".to_string(),
            match_type: RuleMatchType::Process,
            // Stygian AI browser process name. App-name match → cgroup-v2 slice.
            match_value: "stygian".to_string(),
            protocol: None,
            action: RuleAction::Vpn,
            exit_server: None,
            priority: 10,
        });
    }
    if cfg.route_llm_tools {
        out.push(RoutingRule {
            id: "managed-llm-tools".to_string(),
            enabled: true,
            name: "managed:llm_tools".to_string(),
            match_type: RuleMatchType::Process,
            match_value: "bob-runtime".to_string(),
            protocol: None,
            action: RuleAction::Vpn,
            exit_server: None,
            priority: 11,
        });
    }
    out
}

/// The highest-priority enabled rule that names an exit_server drives which
/// server the single active tunnel should connect to (single-active-tunnel v1).
/// Returns None when no enabled rule pins an exit.
pub fn preferred_exit_server(cfg: &Config) -> Option<String> {
    effective_rules(cfg)
        .into_iter()
        .filter(|r| r.enabled)
        .find_map(|r| r.exit_server.filter(|s| !s.is_empty()))
}

/// True if any enabled rule needs a full tunnel (a port-based VPN rule has no
/// IP selector, so the active SA must be remote_ts=0.0.0.0/0 for the mark to
/// have a matching policy).
pub fn requires_full_tunnel(cfg: &Config) -> bool {
    effective_rules(cfg).iter().any(|r| {
        r.enabled
            && r.action == RuleAction::Vpn
            && matches!(r.match_type, RuleMatchType::Port | RuleMatchType::PortRange)
    })
}

/// Build the `nft -f -` ruleset string for the current config + clear/rebuild
/// the `inet privado_routing` table atomically.
pub fn apply_routing_rules(cfg: &Config) {
    let rules = effective_rules(cfg);

    // Collect set members + chain statements.
    let mut vpn_v4: Vec<String> = Vec::new();
    let mut vpn_v6: Vec<String> = Vec::new();
    let mut direct_v4: Vec<String> = Vec::new();
    let mut direct_v6: Vec<String> = Vec::new();
    let mut chain_stmts: Vec<String> = Vec::new();
    let mut need_cgroup_v2 = false;
    let mut need_cgroup_v1 = false;

    for r in &rules {
        if !r.enabled {
            continue;
        }
        let mark_stmt = match r.action {
            RuleAction::Vpn => format!("meta mark set {VPN_FWMARK}"),
            // Explicit direct short-circuits via return so it wins over later
            // broader VPN rules.
            RuleAction::Direct => "meta mark set 0x0 return".to_string(),
        };

        match r.match_type {
            RuleMatchType::Process => {
                if let Some(uid) = r.match_value.strip_prefix("uid:") {
                    if uid.chars().all(|c| c.is_ascii_digit()) && !uid.is_empty() {
                        chain_stmts.push(format!("    meta skuid {uid} {mark_stmt}"));
                    } else {
                        warn!("[routing-rules] skipping process rule '{}' — bad uid", r.name);
                    }
                } else {
                    // App-name form → cgroup-v2 slice path match. The launcher
                    // moves the app's PIDs into privado_vpn.slice.
                    if has_cgroup_v1_netcls() {
                        // Legacy v1: classid-based match (only when net_cls exists).
                        need_cgroup_v1 = true;
                        chain_stmts.push(format!(
                            "    meta cgroup {} {mark_stmt}",
                            u32::from_str_radix(CGROUP_V1_CLASSID.trim_start_matches("0x"), 16)
                                .unwrap_or(0x0010_1600)
                        ));
                    } else {
                        need_cgroup_v2 = true;
                        chain_stmts.push(format!(
                            "    socket cgroupv2 level 1 \"{CGROUP_V2_SLICE}\" {mark_stmt}"
                        ));
                    }
                }
            }
            RuleMatchType::Domain => {
                for ip in resolve_domain(&r.match_value) {
                    let cidr = to_cidr(&ip);
                    let (v4, v6) = match r.action {
                        RuleAction::Vpn => (&mut vpn_v4, &mut vpn_v6),
                        RuleAction::Direct => (&mut direct_v4, &mut direct_v6),
                    };
                    if is_v6(&cidr) { v6.push(cidr); } else { v4.push(cidr); }
                }
            }
            RuleMatchType::IpCidr => {
                let cidr = to_cidr(&r.match_value);
                let (v4, v6) = match r.action {
                    RuleAction::Vpn => (&mut vpn_v4, &mut vpn_v6),
                    RuleAction::Direct => (&mut direct_v4, &mut direct_v6),
                };
                if is_v6(&cidr) { v6.push(cidr); } else { v4.push(cidr); }
            }
            RuleMatchType::Port => {
                if let Some(stmt) = port_stmt(&r.match_value, r.protocol.as_deref(), &mark_stmt) {
                    chain_stmts.push(stmt);
                } else {
                    warn!("[routing-rules] skipping port rule '{}' — bad port", r.name);
                }
            }
            RuleMatchType::PortRange => {
                if let Some(stmt) = port_range_stmt(&r.match_value, r.protocol.as_deref(), &mark_stmt) {
                    chain_stmts.push(stmt);
                } else {
                    warn!("[routing-rules] skipping port_range rule '{}' — bad range", r.name);
                }
            }
        }
    }

    // Provision cgroups as needed before installing the matching rules.
    if need_cgroup_v2 {
        ensure_cgroup_v2_slice();
    }
    if need_cgroup_v1 {
        ensure_cgroup_v1();
    }

    // Single-active-tunnel v1 advisories. A port-based VPN rule has no IP
    // selector, so the active SA must be full-tunnel (remote_ts=0.0.0.0/0) for
    // its fwmark to match an XFRM policy; surface that here. Likewise note the
    // highest-priority enabled exit_server that should drive the one tunnel.
    if requires_full_tunnel(cfg) {
        warn!("[routing-rules] a port→VPN rule is active — its fwmark only matches when the tunnel is FULL (remote_ts=0.0.0.0/0); narrow split selectors will drop it");
    }
    if let Some(exit) = preferred_exit_server(cfg) {
        info!("[routing-rules] preferred exit server (single-active-tunnel v1): {exit}");
    }

    // Render the table. Domain/ip_cidr set lookups come AFTER the chain
    // statements so an explicit direct port/process rule can short-circuit; the
    // set-based direct entries are emitted first inside the set block so they
    // win over the vpn set lookups.
    let mut ruleset = String::new();
    ruleset.push_str(&format!("flush table inet {NFT_TABLE}\n"));
    ruleset.push_str(&format!("table inet {NFT_TABLE} {{\n"));
    ruleset.push_str(&render_set("vpn_dst_v4", "ipv4_addr", &vpn_v4));
    ruleset.push_str(&render_set("vpn_dst_v6", "ipv6_addr", &vpn_v6));
    ruleset.push_str(&render_set("direct_dst_v4", "ipv4_addr", &direct_v4));
    ruleset.push_str(&render_set("direct_dst_v6", "ipv6_addr", &direct_v6));
    ruleset.push_str("  chain output {\n");
    // type route so a mark change re-triggers the route lookup (marked packet
    // then hits the fwmark ip rule → table 1016).
    ruleset.push_str("    type route hook output priority mangle; policy accept;\n");
    // Explicit direct destinations win first.
    if !direct_v4.is_empty() {
        ruleset.push_str("    ip daddr @direct_dst_v4 meta mark set 0x0 return\n");
    }
    if !direct_v6.is_empty() {
        ruleset.push_str("    ip6 daddr @direct_dst_v6 meta mark set 0x0 return\n");
    }
    // Then the per-process / per-port statements (ascending priority order).
    for stmt in &chain_stmts {
        ruleset.push_str(stmt);
        ruleset.push('\n');
    }
    // Finally the VPN destination sets.
    if !vpn_v4.is_empty() {
        ruleset.push_str(&format!("    ip daddr @vpn_dst_v4 meta mark set {VPN_FWMARK}\n"));
    }
    if !vpn_v6.is_empty() {
        ruleset.push_str(&format!("    ip6 daddr @vpn_dst_v6 meta mark set {VPN_FWMARK}\n"));
    }
    ruleset.push_str("  }\n");
    ruleset.push_str("}\n");

    // Ensure the table exists before `flush table` (flush errors on a missing
    // table); create an empty one first, idempotently.
    let _ = Command::new("nft")
        .args(["add", "table", "inet", NFT_TABLE])
        .output();

    match run_nft(&ruleset) {
        Ok(()) => info!(
            "[routing-rules] applied: {} enabled rule(s), {} vpn-dst, {} direct-dst, {} chain stmt(s)",
            rules.iter().filter(|r| r.enabled).count(),
            vpn_v4.len() + vpn_v6.len(),
            direct_v4.len() + direct_v6.len(),
            chain_stmts.len(),
        ),
        Err(e) => warn!("[routing-rules] nft apply failed: {e}"),
    }
}

/// Tear down the entire routing-rule table atomically.
pub fn clear_routing_rules() {
    let out = Command::new("nft")
        .args(["delete", "table", "inet", NFT_TABLE])
        .output();
    match out {
        Ok(o) if o.status.success() => info!("[routing-rules] table inet {NFT_TABLE} removed"),
        // A missing table on disconnect is fine (nothing to clear).
        Ok(_) => {}
        Err(e) => warn!("[routing-rules] nft delete table failed: {e}"),
    }
}

/// Render an nft set block (empty body when there are no members).
fn render_set(name: &str, ty: &str, members: &[String]) -> String {
    if members.is_empty() {
        format!("  set {name} {{ type {ty}; flags interval; }}\n")
    } else {
        format!(
            "  set {name} {{ type {ty}; flags interval; elements = {{ {} }} }}\n",
            members.join(", ")
        )
    }
}

/// Build a single-port chain statement, validating the port number.
fn port_stmt(port: &str, proto: Option<&str>, mark_stmt: &str) -> Option<String> {
    let p: u16 = port.trim().parse().ok()?;
    Some(format!("    {} dport {p} {mark_stmt}", proto_match(proto)))
}

/// Build a port-range chain statement, validating `lo-hi`.
fn port_range_stmt(range: &str, proto: Option<&str>, mark_stmt: &str) -> Option<String> {
    let (lo, hi) = range.split_once('-')?;
    let lo: u16 = lo.trim().parse().ok()?;
    let hi: u16 = hi.trim().parse().ok()?;
    if lo > hi {
        return None;
    }
    Some(format!("    {} dport {lo}-{hi} {mark_stmt}", proto_match(proto)))
}

/// nft L4-proto matcher. None = both tcp and udp.
fn proto_match(proto: Option<&str>) -> String {
    match proto.map(|p| p.to_ascii_lowercase()) {
        Some(ref p) if p == "tcp" => "tcp".to_string(),
        Some(ref p) if p == "udp" => "udp".to_string(),
        _ => "meta l4proto { tcp, udp } th".to_string(),
    }
}

/// Pipe a ruleset into `nft -f -`.
fn run_nft(ruleset: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new("nft")
        .args(["-f", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn nft: {e}"))?;

    child
        .stdin
        .as_mut()
        .ok_or("no stdin for nft")?
        .write_all(ruleset.as_bytes())
        .map_err(|e| format!("write nft ruleset: {e}"))?;

    let out = child.wait_with_output().map_err(|e| format!("wait nft: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn rule(mt: RuleMatchType, val: &str, action: RuleAction, prio: u32) -> RoutingRule {
        RoutingRule {
            id: crate::config::gen_id(),
            enabled: true,
            name: format!("test:{val}"),
            match_type: mt,
            match_value: val.to_string(),
            protocol: None,
            action,
            exit_server: None,
            priority: prio,
        }
    }

    #[test]
    fn port_statements_validate() {
        assert!(port_stmt("443", Some("tcp"), "meta mark set 0x1016").is_some());
        assert!(port_stmt("notaport", None, "x").is_none());
        assert!(port_range_stmt("8000-8100", Some("udp"), "x").is_some());
        // Reversed range is rejected.
        assert!(port_range_stmt("9000-8000", None, "x").is_none());
    }

    #[test]
    fn to_cidr_handles_v4_v6_and_existing() {
        assert_eq!(to_cidr("1.2.3.4"), "1.2.3.4/32");
        assert_eq!(to_cidr("2001:db8::1"), "2001:db8::1/128");
        assert_eq!(to_cidr("10.0.0.0/8"), "10.0.0.0/8");
    }

    #[test]
    fn requires_full_tunnel_only_for_port_vpn() {
        let mut cfg = Config::default();
        cfg.routing_rules = vec![rule(RuleMatchType::IpCidr, "10.0.0.0/8", RuleAction::Vpn, 1)];
        assert!(!requires_full_tunnel(&cfg));
        cfg.routing_rules = vec![rule(RuleMatchType::Port, "443", RuleAction::Vpn, 1)];
        assert!(requires_full_tunnel(&cfg));
        // A direct port rule does not force full tunnel.
        cfg.routing_rules = vec![rule(RuleMatchType::Port, "443", RuleAction::Direct, 1)];
        assert!(!requires_full_tunnel(&cfg));
    }

    #[test]
    fn preferred_exit_picks_highest_priority() {
        let mut cfg = Config::default();
        let mut r_low = rule(RuleMatchType::IpCidr, "1.0.0.0/8", RuleAction::Vpn, 50);
        r_low.exit_server = Some("ams-101.vpn.privado.io".into());
        let mut r_high = rule(RuleMatchType::IpCidr, "2.0.0.0/8", RuleAction::Vpn, 5);
        r_high.exit_server = Some("sin-005.vpn.privado.io".into());
        cfg.routing_rules = vec![r_low, r_high];
        assert_eq!(preferred_exit_server(&cfg).as_deref(), Some("sin-005.vpn.privado.io"));
    }

    #[test]
    fn managed_rules_synthesized_from_toggles() {
        let mut cfg = Config::default();
        cfg.route_llm_browser = true;
        cfg.route_llm_tools = true;
        let rules = effective_rules(&cfg);
        assert!(rules.iter().any(|r| r.name == "managed:llm_browser"));
        assert!(rules.iter().any(|r| r.name == "managed:llm_tools"));
    }

    #[test]
    fn render_set_empty_vs_populated() {
        assert!(render_set("vpn_dst_v4", "ipv4_addr", &[]).contains("flags interval;"));
        let s = render_set("vpn_dst_v4", "ipv4_addr", &["1.2.3.4/32".into()]);
        assert!(s.contains("elements = { 1.2.3.4/32 }"));
    }
}
