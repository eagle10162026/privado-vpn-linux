//! Shared daemon state. Authorization is held in-memory only — it does not
//! persist across daemon restarts, which is intentional: a daemon restart
//! always returns to "no authorization, tear down any live SAs."

use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;

pub const MAX_AUTH_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// The active VPN protocol for the current connection.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ActiveProtocol {
    #[default]
    IKEv2,
    WireGuard,
    OpenVPN,
}

#[derive(Debug, Clone, Default)]
pub struct DaemonState {
    pub authorized:    bool,
    pub authorized_at: Option<SystemTime>,
    pub country:       Option<String>,
    /// Which protocol is currently active for the connection.
    pub active_protocol: ActiveProtocol,
    /// Whether the connection is currently paused (will auto-resume).
    pub paused: bool,
    /// When the pause will expire (if paused).
    pub pause_expires: Option<SystemTime>,
    /// The server hostname for the current/last connection (for reconnect).
    pub current_server: Option<String>,
    /// The resolved remote endpoint IPs for the current connection, captured
    /// at connect time so `on_disconnect` can tear down the exact mangle MARK
    /// rules that `on_connect` installed (instead of being passed an empty
    /// slice and leaking stale per-IP rules).
    pub current_remote_ips: Vec<String>,
}

impl DaemonState {
    pub fn authorization_fresh(&self) -> bool {
        match (self.authorized, self.authorized_at) {
            (true, Some(t)) => SystemTime::now()
                .duration_since(t)
                .map(|d| d < MAX_AUTH_AGE)
                .unwrap_or(false),
            _ => false,
        }
    }

    pub fn authorize(&mut self, country: String) {
        self.authorized = true;
        self.authorized_at = Some(SystemTime::now());
        self.country = Some(country);
        self.paused = false;
        self.pause_expires = None;
    }

    pub fn revoke(&mut self) {
        self.authorized = false;
        self.authorized_at = None;
        self.country = None;
        self.paused = false;
        self.pause_expires = None;
        self.current_server = None;
        self.current_remote_ips = Vec::new();
    }

    pub fn set_paused(&mut self, duration_secs: u64) {
        self.paused = true;
        self.pause_expires = Some(SystemTime::now() + Duration::from_secs(duration_secs));
    }

    pub fn is_paused(&self) -> bool {
        if !self.paused { return false; }
        match self.pause_expires {
            Some(expires) => SystemTime::now() < expires,
            None => false,
        }
    }
}

pub type SharedState = Arc<RwLock<DaemonState>>;
