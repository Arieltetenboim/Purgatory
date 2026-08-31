//! Real-process lifetime check. Ignored in the default gate (starts the workspace server).
//!
//! ```text
//! cargo test -p purgatory-dev-runtime --test session_lifetime -- --ignored --nocapture --test-threads=1
//! ```

use std::time::{Duration, Instant};

use purgatory_dev_runtime::{
    HubCommand, LiveHubSession, ProcessOrigin, ServerState, WorkspacePaths,
};

#[test]
#[ignore]
fn dedicated_server_survives_hub_session_drop() {
    let paths = WorkspacePaths::detect().expect("workspace");
    let mut first = LiveHubSession::open_at(paths.clone(), Instant::now()).expect("open");
    if first.snapshot(Instant::now()).pid.is_some() {
        first.command(HubCommand::Stop, Instant::now());
        wait_state(&mut first, ServerState::Stopped, Duration::from_secs(30))
            .expect("stop leftover");
    }
    first.command(HubCommand::Start, Instant::now());
    let pid = wait_ready(&mut first, Duration::from_secs(90)).expect("first Ready");
    eprintln!("lifetime-test spawned pid {pid}");
    assert_eq!(
        first.snapshot(Instant::now()).process_origin,
        Some(ProcessOrigin::Spawned)
    );
    drop(first);

    assert!(
        pid_alive(pid),
        "server pid {pid} must still be alive after HubSession drop"
    );

    let mut second = LiveHubSession::open_at(paths, Instant::now()).expect("reopen");
    let snap = second.snapshot(Instant::now());
    assert_eq!(snap.pid, Some(pid));
    assert_eq!(snap.process_origin, Some(ProcessOrigin::Adopted));
    assert_ne!(
        snap.server_state,
        ServerState::Ready,
        "reopen must not claim Ready before probe"
    );
    assert!(
        matches!(
            snap.server_state,
            ServerState::Starting | ServerState::Verifying
        ),
        "expected Starting/Verifying, got {:?}",
        snap.server_state
    );

    let pid2 = wait_ready(&mut second, Duration::from_secs(90)).expect("reopen Ready");
    assert_eq!(pid2, pid, "adopted server must keep the same PID");
    second.command(HubCommand::Stop, Instant::now());
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        second.tick(Instant::now());
        if second.snapshot(Instant::now()).server_state == ServerState::Stopped && !pid_alive(pid) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!(
        "server pid {pid} did not stop; state={:?}",
        second.snapshot(Instant::now()).server_state
    );
}

fn wait_ready(session: &mut LiveHubSession, timeout: Duration) -> Result<u32, String> {
    wait_state(session, ServerState::Ready, timeout)?;
    session
        .snapshot(Instant::now())
        .pid
        .ok_or_else(|| "Ready without pid".into())
}

fn wait_state(
    session: &mut LiveHubSession,
    want: ServerState,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        session.tick(Instant::now());
        let snap = session.snapshot(Instant::now());
        if snap.server_state == want {
            return Ok(());
        }
        if want != ServerState::Failed && snap.server_state == ServerState::Failed {
            return Err(format!(
                "Failed while waiting for {want:?}: {}",
                snap.last_failure.unwrap_or_default()
            ));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(format!(
        "timed out waiting for {want:?}; last {:?}",
        session.snapshot(Instant::now()).server_state
    ))
}

fn pid_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("tasklist.exe")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .creation_flags(0x0800_0000)
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .is_some_and(|s| s.contains(&pid.to_string()))
    }
    #[cfg(not(windows))]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
}
