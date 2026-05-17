// PrivadoVPN privileged helper
// Installed setuid root by the deb postinst script.
// Accepts a single subcommand + args. Validates everything strictly.
//
// Exit codes:
//   0 = success
//   1 = bad usage
//   2 = validation failure
//   3 = privileged op failed (stderr contains the reason)
//   4 = not running as root (setuid bit lost)

use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};

const SWANCTL: &str = "/usr/sbin/swanctl";
const SYSTEMCTL: &str = "/usr/bin/systemctl";
const IPTABLES: &str = "/usr/sbin/iptables";
const RESOLVCONF: &str = "/usr/sbin/resolvconf";
const IPSEC_CONF: &str = "/etc/swanctl/conf.d/privado.conf";
const IPSEC_SECRETS: &str = "/etc/swanctl/conf.d/privado.secrets";
const CONN_NAME: &str = "privado";

fn die(code: u8, msg: &str) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::from(code)
}

fn ok(msg: &str) -> ExitCode {
    println!("{msg}");
    ExitCode::from(0)
}

fn ensure_root() -> Result<(), ExitCode> {
    // SAFETY: getuid is a syscall with no side-effects.
    let uid = unsafe { libc_getuid() };
    if uid != 0 {
        return Err(die(4, "helper not running as root (setuid bit missing)"));
    }
    Ok(())
}

extern "C" {
    fn getuid() -> u32;
}
unsafe fn libc_getuid() -> u32 { getuid() }

fn valid_hostname(s: &str) -> bool {
    !s.is_empty() && s.len() <= 253 && s.bytes().all(|b| {
        b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_'
    })
}

fn valid_username(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && s.bytes().all(|b| {
        b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_' || b == b'@'
    })
}

fn valid_password(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.contains('"') && !s.contains('\n') && !s.contains('\r')
}

fn valid_cidr(s: &str) -> bool {
    if s.is_empty() || s.len() > 64 { return false; }
    let (addr, mask) = match s.split_once('/') {
        Some(p) => p,
        None => return false,
    };
    let mask_n: u8 = match mask.parse() { Ok(n) => n, Err(_) => return false };
    if addr.contains(':') {
        if mask_n > 128 { return false; }
        addr.parse::<std::net::Ipv6Addr>().is_ok()
    } else {
        if mask_n > 32 { return false; }
        addr.parse::<std::net::Ipv4Addr>().is_ok()
    }
}

fn valid_ipaddr(s: &str) -> bool {
    s.parse::<std::net::IpAddr>().is_ok()
}

fn write_swanctl(host: &str, username: &str, password: &str, routes: &[String], dns: &[String]) -> Result<(), String> {
    let ts = if routes.is_empty() { "0.0.0.0/0".to_string() } else { routes.join(", ") };
    let dns_line = if dns.is_empty() { String::new() } else {
        format!("\n    dns = {}", dns.join(", "))
    };
    let conf = format!(
        "connections {{\n  {CONN_NAME} {{\n    version = 2\n    remote_addrs = {host}\n    vips = 0.0.0.0, ::{dns_line}\n    proposals = aes256-sha256-modp2048,aes256-sha384-ecp384,default\n    dpd_delay = 30s\n    reauth_time = 0\n    rekey_time = 0\n    local {{\n      auth = eap-mschapv2\n      eap_id = {username}\n    }}\n    remote {{\n      auth = pubkey\n      id = {host}\n    }}\n    children {{\n      {CONN_NAME}-child {{\n        remote_ts = {ts}\n        esp_proposals = aes256-sha256,aes256-sha384,default\n        start_action = none\n        dpd_action = restart\n        close_action = none\n        rekey_time = 0\n      }}\n    }}\n  }}\n}}\n"
    );
    let secrets = format!(
        "secrets {{\n  eap-{CONN_NAME} {{\n    id = {username}\n    secret = \"{password}\"\n  }}\n}}\n"
    );

    let conf_dir = Path::new("/etc/swanctl/conf.d");
    fs::create_dir_all(conf_dir).map_err(|e| format!("create_dir {conf_dir:?}: {e}"))?;
    fs::write(IPSEC_CONF, conf).map_err(|e| format!("write {IPSEC_CONF}: {e}"))?;
    fs::write(IPSEC_SECRETS, secrets).map_err(|e| format!("write {IPSEC_SECRETS}: {e}"))?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(IPSEC_SECRETS, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("chmod secrets: {e}"))?;
    fs::set_permissions(IPSEC_CONF, fs::Permissions::from_mode(0o644))
        .map_err(|e| format!("chmod conf: {e}"))?;
    Ok(())
}

fn run(cmd: &str, args: &[&str]) -> Result<(String, String, bool), String> {
    let out = Command::new(cmd)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .output()
        .map_err(|e| format!("exec {cmd}: {e}"))?;
    Ok((
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.success(),
    ))
}

fn ensure_strongswan_running() -> Result<(), String> {
    let (_, _, active) = run(SYSTEMCTL, &["is-active", "--quiet", "strongswan"])?;
    if !active {
        let (_, stderr, ok) = run(SYSTEMCTL, &["start", "strongswan"])?;
        if !ok { return Err(format!("systemctl start strongswan: {stderr}")); }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    Ok(())
}

fn cmd_connect(args: &[String]) -> ExitCode {
    if args.len() < 5 {
        return die(1, "usage: connect <host> <username> <password-file> <routes-csv> <dns-csv>");
    }
    let host = &args[0];
    let username = &args[1];
    let password_file = &args[2];
    let routes_csv = &args[3];
    let dns_csv = &args[4];

    if !valid_hostname(host) { return die(2, "invalid hostname"); }
    if !valid_username(username) { return die(2, "invalid username"); }

    let password = match fs::read_to_string(password_file) {
        Ok(s) => s.trim_end_matches(['\n', '\r']).to_string(),
        Err(e) => return die(2, &format!("read password file: {e}")),
    };
    if !valid_password(&password) { return die(2, "invalid password (control chars or quotes)"); }

    let routes: Vec<String> = routes_csv.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    for r in &routes {
        if !valid_cidr(r) { return die(2, &format!("invalid CIDR route: {r}")); }
    }
    let dns: Vec<String> = dns_csv.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    for d in &dns {
        if !valid_ipaddr(d) { return die(2, &format!("invalid DNS server: {d}")); }
    }

    if let Err(e) = ensure_strongswan_running() { return die(3, &e); }
    if let Err(e) = write_swanctl(host, username, &password, &routes, &dns) { return die(3, &e); }

    let (_, stderr, loaded) = match run(SWANCTL, &["--load-all"]) {
        Ok(t) => t,
        Err(e) => return die(3, &e),
    };
    if !loaded && !stderr.contains("already loaded") {
        return die(3, &format!("swanctl --load-all: {stderr}"));
    }

    let child = format!("{CONN_NAME}-child");
    let (stdout, stderr, ok_init) = match run(SWANCTL, &["--initiate", "--child", &child]) {
        Ok(t) => t,
        Err(e) => return die(3, &e),
    };
    if !ok_init {
        let msg = stderr.trim().lines().last().unwrap_or(&stdout).to_string();
        return die(3, &format!("swanctl --initiate: {msg}"));
    }
    ok("connected")
}

fn cmd_disconnect() -> ExitCode {
    let _ = run(SWANCTL, &["--terminate", "--ike", CONN_NAME]);
    let _ = run(SWANCTL, &["--terminate", "--child", &format!("{CONN_NAME}-child")]);
    ok("disconnected")
}

fn cmd_status() -> ExitCode {
    let (stdout, _, _) = match run(SWANCTL, &["--list-sas"]) {
        Ok(t) => t,
        Err(e) => return die(3, &e),
    };
    if stdout.contains(CONN_NAME) {
        ok("alive")
    } else {
        die(3, "no active SA")
    }
}

fn cmd_killswitch_on() -> ExitCode {
    let chain = "PRIVADO_KS";
    let _ = run(IPTABLES, &["-N", chain]);
    let _ = run(IPTABLES, &["-F", chain]);
    let rules: &[&[&str]] = &[
        &["-A", chain, "-o", "lo", "-j", "ACCEPT"],
        &["-A", chain, "-m", "policy", "--dir", "out", "--pol", "ipsec", "-j", "ACCEPT"],
        &["-A", chain, "-p", "udp", "--dport", "67:68", "-j", "ACCEPT"],
        &["-A", chain, "-p", "udp", "--dport", "53", "-j", "ACCEPT"],
        &["-A", chain, "-p", "udp", "--dport", "500", "-j", "ACCEPT"],
        &["-A", chain, "-p", "udp", "--dport", "4500", "-j", "ACCEPT"],
        &["-A", chain, "-p", "esp", "-j", "ACCEPT"],
        &["-A", chain, "-j", "REJECT", "--reject-with", "icmp-net-unreachable"],
    ];
    for r in rules {
        let (_, stderr, ok_r) = match run(IPTABLES, r) {
            Ok(t) => t,
            Err(e) => return die(3, &e),
        };
        if !ok_r { return die(3, &format!("iptables {r:?}: {stderr}")); }
    }
    let (_, _, present) = match run(IPTABLES, &["-C", "OUTPUT", "-j", chain]) {
        Ok(t) => t,
        Err(e) => return die(3, &e),
    };
    if !present {
        let (_, stderr, ok_i) = match run(IPTABLES, &["-I", "OUTPUT", "1", "-j", chain]) {
            Ok(t) => t,
            Err(e) => return die(3, &e),
        };
        if !ok_i { return die(3, &format!("iptables -I OUTPUT: {stderr}")); }
    }
    ok("killswitch-on")
}

fn cmd_killswitch_off() -> ExitCode {
    let chain = "PRIVADO_KS";
    let _ = run(IPTABLES, &["-D", "OUTPUT", "-j", chain]);
    let _ = run(IPTABLES, &["-F", chain]);
    let _ = run(IPTABLES, &["-X", chain]);
    ok("killswitch-off")
}

fn cmd_dns_set(args: &[String]) -> ExitCode {
    if args.is_empty() { return die(1, "usage: dns-set <ip,ip,...>"); }
    let dns: Vec<String> = args[0].split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    if dns.is_empty() { return die(2, "no DNS servers given"); }
    for d in &dns {
        if !valid_ipaddr(d) { return die(2, &format!("invalid IP: {d}")); }
    }
    let content = dns.iter().map(|d| format!("nameserver {d}")).collect::<Vec<_>>().join("\n") + "\n";
    if Path::new(RESOLVCONF).exists() {
        let mut child = match Command::new(RESOLVCONF)
            .args(["-a", "privado-ct", "-m", "0"])
            .stdin(Stdio::piped())
            .env_clear()
            .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
            .spawn() {
            Ok(c) => c,
            Err(e) => return die(3, &format!("resolvconf spawn: {e}")),
        };
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(content.as_bytes()) {
                return die(3, &format!("resolvconf stdin: {e}"));
            }
        }
        match child.wait() {
            Ok(s) if s.success() => ok("dns-set"),
            Ok(s) => die(3, &format!("resolvconf exit: {s}")),
            Err(e) => die(3, &format!("resolvconf wait: {e}")),
        }
    } else {
        // Fallback: write /etc/resolv.conf directly (atomically)
        let tmp = "/etc/resolv.conf.privado-tmp";
        if let Err(e) = fs::write(tmp, &content) {
            return die(3, &format!("write {tmp}: {e}"));
        }
        if let Err(e) = fs::rename(tmp, "/etc/resolv.conf") {
            return die(3, &format!("rename resolv.conf: {e}"));
        }
        ok("dns-set")
    }
}

fn cmd_dns_clear() -> ExitCode {
    if Path::new(RESOLVCONF).exists() {
        let _ = run(RESOLVCONF, &["-d", "privado-ct"]);
    }
    ok("dns-clear")
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        return die(1, "usage: privado-vpn-helper <connect|disconnect|status|killswitch-on|killswitch-off|dns-set|dns-clear> [args...]");
    }
    if let Err(c) = ensure_root() { return c; }

    let sub = args[1].as_str();
    let rest: Vec<String> = args[2..].to_vec();
    match sub {
        "connect" => cmd_connect(&rest),
        "disconnect" => cmd_disconnect(),
        "status" => cmd_status(),
        "killswitch-on" => cmd_killswitch_on(),
        "killswitch-off" => cmd_killswitch_off(),
        "dns-set" => cmd_dns_set(&rest),
        "dns-clear" => cmd_dns_clear(),
        _ => die(1, &format!("unknown subcommand: {sub}")),
    }
}
