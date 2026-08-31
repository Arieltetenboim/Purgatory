mod app;
mod assets;
mod camera_follow;
mod debug;
mod frontend;
mod input;
mod interp;
mod jitter_forensics;
mod lifecycle;
mod local_presentation;
mod map_fade;
mod network;
mod platform;
mod prediction;
mod renderer;
mod replica;
mod ui_runtime;

use tracing_subscriber::EnvFilter;

fn main() {
    init_tracing();
    let _ = (
        purgatory_common::version(),
        purgatory_protocol::version(),
        purgatory_content::version(),
        purgatory_simulation::version(),
    );
    println!(
        "PURGATORY client bootstrap OK {} protocol_version={}",
        purgatory_common::identity(),
        purgatory_protocol::PROTOCOL_VERSION
    );
    crate::debug::agent_log::emit("boot", "main.rs:main", "client_boot", "{\"ok\":true}");
    if let Err(err) = app::run() {
        eprintln!("PURGATORY client error: {err}");
        std::process::exit(1);
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,quinn=warn,quinn_proto=warn,rustls=warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}
