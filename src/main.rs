//! PrivadoVPN — single-binary Linux client (CLI + daemon).
//!
//! Dispatch:
//!   privado-vpn daemon         → run the backend (systemd-managed, root)
//!   privado-vpn connect ...    → thin client → daemon HTTP API
//!   privado-vpn disconnect ... → thin client → daemon HTTP API
//!   privado-vpn status         → thin client → daemon HTTP API
//!   privado-vpn servers        → thin client → daemon HTTP API
//!   privado-vpn config / set / login / logout — local config helpers
//!
//! The Tauri desktop app (`privado-vpn-ui`) also talks to this daemon
//! via the same HTTP API at 127.10.0.18:1600. CLI and UI always show
//! the same state — one `systemctl stop privado-vpn` kills everything.

mod api;
mod config;
mod client;
mod daemon;
pub(crate) mod routing;

use clap::{Parser, Subcommand};
use std::io::Write;
use tracing::error;

#[derive(Parser)]
#[command(name = "privado-vpn")]
#[command(about = "PrivadoVPN — native Linux VPN client (CLI + systemd daemon)")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the backend daemon. Listens on /run/privado-vpn.sock + HTTP 127.10.0.18:1600.
    /// Started by systemd: `systemctl start privado-vpn.service`.
    Daemon,
    /// Connect to a VPN server (requires --pin; see /etc/bobai/VPN.lock)
    Connect {
        /// Server hostname (e.g. "ams-101.vpn.privado.io") or country code (nl, sg, mx)
        #[arg(short, long)]
        server: Option<String>,
        /// Country code (used when --server is not specified)
        #[arg(short, long)]
        country: Option<String>,
        /// Authorization PIN
        #[arg(long)]
        pin: Option<String>,
    },
    /// Disconnect (requires --pin)
    Disconnect {
        #[arg(long)]
        pin: Option<String>,
    },
    /// Show live connection status
    Status,
    /// List provisioned server locations
    Servers,
    /// Log in to PrivadoVPN portal (stores credentials for the UI/CLI)
    Login {
        #[arg(short, long)] username: Option<String>,
        #[arg(short, long)] password: Option<String>,
    },
    /// Log out and clear saved portal credentials
    Logout,
    /// Show the on-disk config
    Config,
    /// Set a single config field
    Set {
        key:   String,
        value: String,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        None => {
            // No subcommand — print help since the GUI is a separate binary (privado-vpn-ui)
            println!("PrivadoVPN CLI — use `privado-vpn --help` for commands.");
            println!("To launch the desktop app: privado-vpn-ui");
            println!("To check status: privado-vpn status");
        }
        Some(Commands::Daemon) => {
            init_tracing();
            let rt = tokio::runtime::Runtime::new()
                .expect("failed to build tokio runtime for daemon");
            if let Err(e) = rt.block_on(daemon::run()) {
                error!("daemon exited: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Connect { server, country, pin }) => {
            init_tracing();
            cmd_connect(server, country, pin);
        }
        Some(Commands::Disconnect { pin }) => { init_tracing(); cmd_disconnect(pin); }
        Some(Commands::Status)           => { init_tracing(); cmd_status(); }
        Some(Commands::Servers)          => { init_tracing(); cmd_servers(); }
        Some(Commands::Login { username, password }) => {
            init_tracing();
            let rt = tokio::runtime::Runtime::new().expect("tokio");
            rt.block_on(cmd_login(username, password));
        }
        Some(Commands::Logout)            => { init_tracing(); cmd_logout(); }
        Some(Commands::Config)            => { init_tracing(); cmd_show_config(); }
        Some(Commands::Set { key, value }) => { init_tracing(); cmd_set(&key, &value); }
    }
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "privado_vpn=info".into()),
        )
        .with_target(false)
        .try_init();
}

// ─── CLI handlers ──────────────────────────────────────────────────────────

fn cmd_connect(server: Option<String>, country: Option<String>, pin: Option<String>) {
    let pin = require_pin(pin);
    let cfg = config::load_config().unwrap_or_default();

    // Determine what to connect to: explicit server > explicit country > config preferred > "nl"
    let target = server
        .or_else(|| country.map(|c| c.to_lowercase()))
        .or(cfg.preferred_country)
        .unwrap_or_else(|| "nl".to_string());

    match client::connect_vpn(&pin, &target) {
        Ok(s) => print_status(&s),
        Err(e) => { error!("{e}"); std::process::exit(1); }
    }
}

fn cmd_disconnect(pin: Option<String>) {
    let pin = require_pin(pin);
    match client::disconnect_vpn(&pin) {
        Ok(_) => println!("Disconnected."),
        Err(e) => { error!("{e}"); std::process::exit(1); }
    }
}

fn cmd_status() {
    match client::status() {
        Ok(s)  => print_status(&s),
        Err(e) => { error!("{e}"); std::process::exit(1); }
    }
}

fn cmd_servers() {
    match client::servers() {
        Ok(entries) => {
            println!("{:<4}  {:<20}  {}", "Code", "Display", "Remote");
            println!("{}", "-".repeat(70));
            for e in &entries {
                println!("{:<4}  {:<20}  {}", e.country_code, e.display, e.remote_host);
            }
        }
        Err(e) => { error!("{e}"); std::process::exit(1); }
    }
}

fn require_pin(pin: Option<String>) -> String {
    match pin {
        Some(p) if !p.is_empty() => p,
        _ => {
            error!("--pin <PIN> is required (see /etc/bobai/VPN.lock).");
            std::process::exit(2);
        }
    }
}

fn print_status(s: &daemon::proto::VpnStatus) {
    if s.connected {
        println!("Status:      Connected");
        println!("Country:     {}", s.country.as_deref().unwrap_or("?"));
        println!("Server:      {}", s.server.as_deref().unwrap_or("?"));
        println!("Remote IP:   {}", s.remote_ip.as_deref().unwrap_or("?"));
        println!("Virtual IP:  {}", s.local_vip.as_deref().unwrap_or("?"));
        println!("Tunnel:      {}", if s.full_tunnel { "full" } else { "split" });
        println!("Traffic:     {} bytes in / {} bytes out", s.bytes_in, s.bytes_out);
        println!("Duration:    {} s", s.duration_secs);
    } else {
        println!("Status:      Disconnected");
    }
    println!("Authorized:  {}", if s.authorized { "yes" } else { "no" });
    if let Some(at) = &s.authorized_at {
        println!("Auth since:  {at}");
    }
}

// ─── Local config helpers (no daemon round-trip) ────────────────────────────

async fn cmd_login(username: Option<String>, password: Option<String>) {
    let user = username.unwrap_or_else(|| prompt_input("Privado username"));
    let pass = password.unwrap_or_else(|| prompt_password("Privado password"));
    if user.is_empty() || pass.is_empty() {
        error!("Username and password are required");
        std::process::exit(1);
    }
    let mut a = api::PrivadoApi::new();
    match a.login(&user, &pass).await {
        Ok(resp) => {
            let token = config::SavedToken {
                access_token:  resp.access_token.clone().unwrap_or_default(),
                refresh_token: resp.refresh_token.clone(),
                expires_at:    resp.expires_at(),
            };
            if let Err(e) = config::save_token(&token) { error!("save token: {e}"); }
            if let Err(e) = config::save_credentials(&user, &pass) { error!("save creds: {e}"); }
            let mut cfg = config::load_config().unwrap_or_default();
            cfg.username = user;
            cfg.password = pass;
            if let Err(e) = config::save_config(&cfg) { error!("save config: {e}"); }
            println!("Login successful. Portal credentials saved.");
            let _ = std::io::stdout().flush();
        }
        Err(e) => { error!("Login failed: {e}"); std::process::exit(1); }
    }
}

fn prompt_input(label: &str) -> String {
    eprint!("{label}: ");
    let _ = std::io::stderr().flush();
    let mut s = String::new();
    let _ = std::io::stdin().read_line(&mut s);
    s.trim().to_string()
}

fn prompt_password(label: &str) -> String {
    eprint!("{label}: ");
    let _ = std::io::stderr().flush();
    let _ = std::process::Command::new("stty").arg("-echo").status();
    let mut s = String::new();
    let _ = std::io::stdin().read_line(&mut s);
    let _ = std::process::Command::new("stty").arg("echo").status();
    eprintln!();
    s.trim().to_string()
}

fn cmd_logout() {
    config::clear_credentials();
    println!("Logged out. Portal credentials cleared.");
}

fn cmd_show_config() {
    let cfg = config::load_config().unwrap_or_default();
    println!("Configuration ({})\n", config::config_path().display());
    println!("  username:          {}", if cfg.username.is_empty() { "(not set)" } else { &cfg.username });
    println!("  preferred_country: {}", cfg.preferred_country.as_deref().unwrap_or("(auto)"));
    println!("  preferred_city:    {}", cfg.preferred_city.as_deref().unwrap_or("(auto)"));
    println!("  split_tunnel:      {}", cfg.split_tunnel);
    println!("  split_domains:     {}", if cfg.split_domains.is_empty() { "(none)".into() } else { format!("{} domains", cfg.split_domains.len()) });
    println!("  kill_switch:       {}", cfg.kill_switch);
    println!("  auto_connect:      {}", cfg.auto_connect);
    println!("  dns_servers:       {}", cfg.dns_servers.join(", "));
}

fn cmd_set(key: &str, value: &str) {
    let mut cfg = config::load_config().unwrap_or_default();
    match key {
        "preferred_country" | "country" => {
            cfg.preferred_country = Some(value.to_lowercase());
            println!("Set preferred country to {}", value.to_lowercase());
        }
        "preferred_city" | "city" => {
            cfg.preferred_city = Some(value.to_string());
            println!("Set preferred city to {value}");
        }
        "kill_switch" | "killswitch" => {
            cfg.kill_switch = matches!(value, "true" | "1" | "on");
            println!("Kill switch: {}", if cfg.kill_switch { "enabled" } else { "disabled" });
        }
        "auto_connect" | "autoconnect" => {
            cfg.auto_connect = matches!(value, "true" | "1" | "on");
            println!("Auto-connect: {}", if cfg.auto_connect { "enabled" } else { "disabled" });
        }
        "dns" => {
            cfg.dns_servers = value.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            println!("DNS servers: {}", cfg.dns_servers.join(", "));
        }
        _ => {
            error!("Unknown config key: {key}");
            println!("Valid keys: preferred_country, preferred_city, kill_switch, auto_connect, dns");
            return;
        }
    }
    if let Err(e) = config::save_config(&cfg) { error!("save failed: {e}"); }
}
