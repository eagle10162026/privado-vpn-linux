//! Shared daemon state. Authorization is held in-memory only — it does not
//! persist across daemon restarts, which is intentional: a daemon restart
//! always returns to "no authorization, tear down any live SAs."

use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;

pub const MAX_AUTH_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// The active VPN protocol for the current connection.
#[derive(Debug, Clone, PartialEq)]
pub enum ActiveProtocol {
    IKEv2,
    WireGuard,
    OpenVPN,
}

impl Default for ActiveProtocol {
    fn default() -> Self { ActiveProtocol::IKEv2 }
}

#[derive(Debug, Clone)]
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
}

impl Default for DaemonState {
    fn default() -> Self {
        Self {
            authorized: false,
            authorized_at: None,
            country: None,
            active_protocol: ActiveProtocol::default(),
            paused: false,
            pause_expires: None,
            current_server: None,
        }
    }
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
