//! Internal guardian loop. Every 30 seconds, if any `privado-*` IKE_SA is
//! ESTABLISHED but the daemon has no fresh authorization marker, tear them
//! down. This is the in-daemon replacement for the old
//! `vpn-guardian.timer`/`vpn-guardian.sh` glue.

use crate::daemon::state::SharedState;
use crate::daemon::swanctl;
use std::time::Duration;
use tracing::{info, warn};

pub async fn run_loop(state: SharedState) {
    info!("[guardian] running every 30s");
    let mut tick = tokio::time::interval(Duration::from_secs(30));
    tick.tick().await; // skip immediate fire
    loop {
        tick.tick().await;
        let count = swanctl::count_established_privado_sas().await;
        let fresh = state.read().await.authorization_fresh();
        if count > 0 && !fresh {
            warn!("[guardian] {count} unauthorized SA(s) — terminating");
            swanctl::terminate_all_privado().await;
            state.write().await.revoke();
        }
    }
}
