#![cfg_attr(not(feature = "dev-diagnostics"), allow(dead_code))]

mod app;
mod asset_runtime;
mod assets;
mod camera_follow;
mod character_assets;
mod character_presentation;
mod choice_bubble;
#[cfg(feature = "dev-diagnostics")]
mod debug;
mod dialogue_runtime;
mod display;
#[cfg(feature = "dev-diagnostics")]
mod frontend;
mod headwear_proof;
mod input;
mod interp;
#[cfg(feature = "dev-diagnostics")]
mod jitter_forensics;
mod lifecycle;
mod local_presentation;
mod map_fade;
mod network;
mod npc_presentation;
mod platform;
mod prediction;
mod renderer;
mod replica;
mod skeleton_debug;
mod speech_bubble;
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
        "PURGATORY client bootstrap OK {} protocol_version={} {}",
        purgatory_common::identity(),
        purgatory_protocol::PROTOCOL_VERSION,
        if cfg!(feature = "dev-diagnostics") {
            "dev-diagnostics"
        } else {
            "shipping"
        }
    );
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
