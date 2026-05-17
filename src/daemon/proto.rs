//! Wire protocol between the in-binary daemon and its clients (the CLI
//! subcommands of the same binary, and the GTK GUI in the same binary).
//!
//! Transport: newline-delimited JSON over Unix datagram socket at
//! `/run/privado-vpn.sock`. One request → one response. Each CLI/GUI call
//! opens a fresh connection.

use serde::{Deserialize, Serialize};

pub const SOCKET_PATH: &str = "/run/privado-vpn.sock";

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Status,
    Servers,
    Connect { pin: String, country: String },
    Disconnect { pin: String },
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct VpnStatus {
    pub connected: bool,
    #[serde(default)] pub server: Option<String>,
    #[serde(default)] pub remote_ip: Option<String>,
    #[serde(default)] pub local_vip: Option<String>,
    #[serde(default)] pub bytes_in: u64,
    #[serde(default)] pub bytes_out: u64,
    #[serde(default)] pub duration_secs: u64,
    #[serde(default)] pub full_tunnel: bool,
    /// Country code (nl|sg|mx) that the daemon authorized — None when not connected.
    #[serde(default)] pub country: Option<String>,
    /// True if a fresh authorization marker is held (24h sliding window).
    #[serde(default)] pub authorized: bool,
    /// ISO 8601 timestamp of when the current authorization was granted.
    #[serde(default)] pub authorized_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProvisionedServer {
    pub country_code: String,
    pub display: String,
    pub remote_host: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    BadPin,
    BadCountry,
    BadRequest,
    StrongswanError,
    Internal,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Response {
    Ok      { status: VpnStatus },
    Servers { entries: Vec<ProvisionedServer> },
    Err     { code: ErrorCode, message: String },
}
