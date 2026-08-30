//! Headless load-test harness for PURGATORY.
//!
//! Phase 5.7 headless bot client. Connects multiple bot sessions to the server
//! using the same protocol as the native client.

pub mod aggregate;
pub mod behavior;
pub mod cert;
pub mod classify;
pub mod cli;
pub mod controller;
pub mod dashboard;
pub mod endpoint;
pub mod log;
pub mod metrics;
pub mod prng;
pub mod probe;
pub mod scenario;
pub mod server_metrics;
pub mod session;

pub use behavior::{BotAction, BotBehavior, BotProfile};
pub use cert::DevOnlySkipServerVerification;
pub use classify::RunStatus;
pub use cli::Cli;
pub use controller::Controller;
pub use dashboard::Dashboard;
pub use endpoint::SharedEndpoint;
pub use log::RunLog;
pub use metrics::{BotMetrics, HarnessMetrics};
pub use prng::Lcg;
pub use probe::{PROBE_DEV_LOGIN, run_probe};
pub use scenario::{LoadKind, LoadScenario, Scenario, ValidationPreset};
pub use server_metrics::ServerMetricsPoller;
pub use session::{BotSession, SessionState};

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn client_build() -> String {
    format!("purgatory-load-{}", version())
}

/// Shared entry for `purgatory-load` and the `purgatory-bot-client` alias.
pub fn run_from_env() -> ! {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let _ = rustls::crypto::ring::default_provider().install_default();
    let (cli, spec) = match Cli::parse_resolved() {
        Ok(v) => v,
        Err(err) => {
            eprintln!("purgatory-load: {err}");
            std::process::exit(2);
        }
    };

    if cli.print_server_env {
        let mut spec = spec;
        if let Some(root) = &cli.persist_root {
            spec = spec.with_explicit_persist_root(root);
        }
        match serde_json::to_string(&spec.recommended_server_env()) {
            Ok(json) => {
                println!("{json}");
                std::process::exit(0);
            }
            Err(err) => {
                eprintln!("purgatory-load --print-server-env: {err}");
                std::process::exit(2);
            }
        }
    }

    if cli.probe {
        match crate::run_probe(cli.server) {
            Ok(()) => {
                println!("PURGATORY probe OK {}", cli.server);
                std::process::exit(0);
            }
            Err(err) => {
                eprintln!("purgatory-load --probe: {err}");
                std::process::exit(1);
            }
        }
    }

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = Arc::clone(&shutdown);
    ctrlc::set_handler(move || {
        if shutdown_clone.load(Ordering::Relaxed) {
            std::process::exit(130);
        }
        shutdown_clone.store(true, Ordering::Relaxed);
        eprintln!("\nShutting down...");
    })
    .ok();

    println!(
        "PURGATORY load harness {}  phase {}",
        purgatory_common::version(),
        purgatory_common::phase()
    );
    println!(
        "kind={:?} preset={:?} connect={:?} count={} synthetic={} seed={} run_id={} duration={}s timeout={}s isolate_persist={}",
        spec.kind,
        spec.preset,
        spec.connect,
        spec.bot_count,
        spec.validation.synthetic_entities,
        spec.seed,
        spec.run_id,
        spec.duration_secs,
        spec.timeout_secs,
        spec.isolate_persist
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("purgatory-load")
        .build()
        .expect("tokio runtime");

    let status = runtime.block_on(async {
        let mut controller = Controller::new(cli, spec, shutdown)?;
        controller.run().await
    });

    match status {
        Ok(s) => {
            let code = match s {
                RunStatus::Failed => 1,
                RunStatus::Aborted => 130,
                _ => 0,
            };
            std::process::exit(code);
        }
        Err(err) => {
            eprintln!("purgatory-load error: {err}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_nonempty() {
        assert!(!version().is_empty());
    }

    #[test]
    fn client_build_has_prefix() {
        assert!(client_build().starts_with("purgatory-load-"));
    }

    #[test]
    fn workspace_crates_are_linked() {
        assert!(!purgatory_common::version().is_empty());
        assert!(!purgatory_protocol::version().is_empty());
        assert!(!purgatory_simulation::version().is_empty());
    }
}
