mod app;
mod debug;
mod input;
mod network;
mod platform;
mod renderer;

use tracing_subscriber::EnvFilter;

fn main() {
    init_tracing();
    let _ = (
        purgatory_common::version(),
        purgatory_protocol::version(),
        purgatory_content::version(),
        purgatory_simulation::version(),
    );
    println!("PURGATORY client bootstrap OK");
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
