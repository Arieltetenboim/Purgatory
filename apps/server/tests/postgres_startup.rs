//! Process-level startup of the real `purgatory-server` binary.
//!
//! `main` builds a Tokio runtime and then opens PostgreSQL. The synchronous
//! client must be opened on the persistence worker. These tests use a
//! disposable database and never target `Purgatory_dev`.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use purgatory_persistence::{PersistenceService, PostgresSettings, drop_test_schema};

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn postgres_12c_server_process_starts_after_postgres_opens_and_rejects_invalid_settings() {
    let url = std::env::var("PURGATORY_TEST_DATABASE_URL").unwrap_or_default();
    assert!(
        !url.trim().is_empty(),
        "PURGATORY_TEST_DATABASE_URL is unset, so this PostgreSQL test was not executed"
    );
    assert!(
        !url.to_ascii_lowercase().contains("purgatory_dev"),
        "refusing a startup test aimed at Purgatory_dev"
    );
    assert_udp_port_free(5001);
    let schema = format!(
        "p12a_proc_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_millis()
    );
    let settings = PostgresSettings::for_tests(url, schema).expect("disposable schema");
    PersistenceService::bootstrap_postgresql(&settings).expect("initialize disposable database");

    let mut child = server_command()
        .env("PURGATORY_DATABASE_URL", &settings.url)
        .env("PURGATORY_DATABASE_SCHEMA", &settings.schema)
        .env("PURGATORY_DEPLOYMENT_ID", &settings.deployment_id)
        .env_remove("PURGATORY_SHUTDOWN_FILE")
        .env_remove("PURGATORY_DATABASE_MIGRATION_URL")
        .env_remove("PURGATORY_DATABASE_ADMIN_URL")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("server process");
    let (stdout, stderr) = pipes(&mut child);
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut saw_listening = false;
    while Instant::now() < deadline {
        let out = stdout.lock().expect("stdout").clone();
        if out.contains("network listening on")
            && out.contains("PURGATORY persist backend=postgresql")
        {
            saw_listening = true;
            let _ = child.kill();
            break;
        }
        if let Some(status) = child.try_wait().expect("try_wait") {
            let _ = drop_test_schema(&settings);
            panic!(
                "server exited before listening: {status}\nstdout:\n{out}\nstderr:\n{}",
                stderr.lock().expect("stderr")
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if !saw_listening {
        let _ = child.kill();
    }
    let _ = child.wait();
    let out = stdout.lock().expect("stdout").clone();
    let err = stderr.lock().expect("stderr").clone();
    let _ = drop_test_schema(&settings);
    assert!(saw_listening, "timed out\nstdout:\n{out}\nstderr:\n{err}");
    assert!(
        !err.contains("Cannot start a runtime"),
        "synchronous postgres client panicked inside Tokio:\n{err}"
    );

    let mut invalid = server_command()
        .env(
            "PURGATORY_DATABASE_URL",
            "postgresql://purgatory_dev:invalid@127.0.0.1:1/purgatory_12a_test?sslmode=disable",
        )
        .env("PURGATORY_DATABASE_SCHEMA", "p12a_startup_invalid")
        .env("PURGATORY_DEPLOYMENT_ID", "p12a-startup-invalid")
        .env_remove("PURGATORY_DATABASE_MIGRATION_URL")
        .env_remove("PURGATORY_DATABASE_ADMIN_URL")
        .env_remove("PURGATORY_SHUTDOWN_FILE")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("invalid server process");
    let (bad_out, bad_err) = pipes(&mut invalid);
    let invalid_deadline = Instant::now() + Duration::from_secs(20);
    let finished = loop {
        if let Some(status) = invalid.try_wait().expect("invalid startup wait") {
            break status;
        }
        if Instant::now() >= invalid_deadline {
            let _ = invalid.kill();
            let _ = invalid.wait();
            panic!(
                "invalid startup did not exit\nstdout:\n{}\nstderr:\n{}",
                bad_out.lock().expect("stdout"),
                bad_err.lock().expect("stderr")
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let bad_stdout = bad_out.lock().expect("stdout").clone();
    let bad_stderr = bad_err.lock().expect("stderr").clone();
    assert!(
        !bad_stdout.contains("network listening"),
        "invalid settings reported listening\n{bad_stdout}\n{bad_stderr}"
    );
    assert!(
        !bad_stderr.contains("Cannot start a runtime"),
        "invalid settings panicked inside Tokio:\n{bad_stderr}"
    );
    assert!(
        bad_stderr.contains("persistence open"),
        "invalid settings did not report a persistence failure\n{bad_stdout}\n{bad_stderr}"
    );
    assert!(
        !finished.success(),
        "invalid settings exited successfully\n{bad_stdout}\n{bad_stderr}"
    );
}

fn server_command() -> Command {
    let mut dir = std::env::current_exe().expect("test executable");
    dir.pop();
    dir.pop();
    let windows = dir.join("purgatory-server.exe");
    let exe = if windows.is_file() {
        windows
    } else {
        dir.join("purgatory-server")
    };
    assert!(
        exe.is_file(),
        "purgatory-server executable is missing at {}",
        exe.display()
    );
    Command::new(exe)
}

fn assert_udp_port_free(port: u16) {
    let socket = std::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, port));
    socket
        .unwrap_or_else(|err| panic!("UDP port {port} must be free for the server process: {err}"));
}

fn pipes(child: &mut std::process::Child) -> (Arc<Mutex<String>>, Arc<Mutex<String>>) {
    let stdout = Arc::new(Mutex::new(String::new()));
    let stderr = Arc::new(Mutex::new(String::new()));
    pump(child.stdout.take().expect("stdout"), stdout.clone());
    pump(child.stderr.take().expect("stderr"), stderr.clone());
    (stdout, stderr)
}

fn pump(mut pipe: impl Read + Send + 'static, sink: Arc<Mutex<String>>) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 512];
        loop {
            match pipe.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => sink
                    .lock()
                    .expect("pipe")
                    .push_str(&String::from_utf8_lossy(&buf[..n])),
            }
        }
    });
}
