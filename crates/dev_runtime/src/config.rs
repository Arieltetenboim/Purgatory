//! Timing and caps copied from `tools/dev/` — do not retune for Slice 1.

use std::time::Duration;

pub const LISTEN_HOST: &str = "127.0.0.1";
pub const LISTEN_PORT: u16 = 5001;
pub const METRICS_PORT: u16 = 5002;

pub const SERVER_PACKAGE: &str = "purgatory-server";
pub const CLIENT_PACKAGE: &str = "purgatory-client";
pub const LOAD_PACKAGE: &str = "purgatory-bot-client";
pub const LOAD_BIN: &str = "purgatory-load";
pub const SERVER_STEM: &str = "purgatory-server";
pub const CLIENT_STEM: &str = "purgatory-client";
pub const LOAD_STEM: &str = "purgatory-load";
pub const LOAD_ADMISSION_CAP: u16 = 256;

pub const CLIENT_STAGGER: Duration = Duration::from_millis(140);

pub const READY_TIMEOUT: Duration = Duration::from_secs(45);
pub const PROBE_RETRY: Duration = Duration::from_secs(1);
pub const RECOVERY_INTERVAL: Duration = Duration::from_secs(5);
pub const METRICS_CACHE: Duration = Duration::from_millis(400);
pub const LISTENER_CACHE: Duration = Duration::from_millis(800);
pub const METRICS_RECV_TIMEOUT: Duration = Duration::from_millis(150);
pub const LIFECYCLE_IDLE: Duration = Duration::from_millis(1000);
pub const LIFECYCLE_FAST: Duration = Duration::from_millis(250);

/// Incoming stdout pump drop-oldest cap (`PurgatoryStreamPump.UiCap`).
pub const PUMP_QUEUE_CAP: usize = 4000;
/// Max lines drained from the pump per lifecycle tick (`DrainUi(500)`).
pub const UI_LOG_DRAIN: usize = 500;
/// In-process activity ring (expanded window cap).
pub const ACTIVITY_LOG_CAP: usize = 4000;
/// Lines shown in the main Hub activity view (`LogBox` MaxLines 180).
pub const ACTIVITY_VIEW_LINES: usize = 180;

pub const HUB_MUTEX_NAME: &str = "Local\\PurgatoryDevHub";
pub const POWERSHELL_MUTEX_NAME: &str = "Local\\PurgatoryDevLauncher";
