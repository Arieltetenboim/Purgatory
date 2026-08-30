//! One-shot Hello/Welcome connection probe for Developer Tools readiness.
//!
//! Uses the existing Quinn client stack and protocol v10. A successful probe
//! follows the normal DEV login / persistence / enter path (`dev.probe`) and
//! may therefore create or restore that character. That is documented debt,
//! not a dedicated health protocol.

use std::net::SocketAddr;
use std::time::Duration;

use purgatory_common::DevLogin;

use crate::behavior::BotProfile;
use crate::endpoint::SharedEndpoint;
use crate::session::BotSession;

/// Reserved DEV login for Developer Tools readiness probes.
pub const PROBE_DEV_LOGIN: &str = "dev.probe";

/// Establish a real session, wait for Welcome, disconnect. No gameplay loop.
pub fn run_probe(server: SocketAddr) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .thread_name("purgatory-probe")
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;
    runtime.block_on(probe_once(server))
}

async fn probe_once(server: SocketAddr) -> Result<(), String> {
    let _ = DevLogin::parse(PROBE_DEV_LOGIN).map_err(|e| format!("probe login: {e:?}"))?;
    let shared = SharedEndpoint::new()?;
    let mut session = BotSession::new(0, BotProfile::Idle, 0);
    let result = session
        .connect_with_login(shared.endpoint(), server, PROBE_DEV_LOGIN)
        .await;
    session.close_and_wait(Duration::from_secs(5)).await;
    shared.close();
    let _ = tokio::time::timeout(Duration::from_secs(5), shared.endpoint().wait_idle()).await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    #[test]
    fn probe_login_is_valid_dev_login() {
        DevLogin::parse(PROBE_DEV_LOGIN).expect("dev.probe must parse");
    }

    #[test]
    fn probe_fails_when_nothing_listens() {
        let addr: SocketAddr = "127.0.0.1:59997".parse().expect("addr");
        let err = run_probe(addr).expect_err("probe must fail without a server");
        assert!(!err.is_empty(), "probe error should explain the failure");
    }
}
