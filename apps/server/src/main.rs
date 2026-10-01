mod network;

use std::time::Duration;

use purgatory_simulation::{SimulationClock, TICK_RATE_HZ, World};

fn main() {
    init_tracing();
    if let Some(result) = database_admin_command() {
        if let Err(err) = result {
            let err = purgatory_persistence::redact_connection_text(&err);
            eprintln!("PURGATORY database error: {err}");
            std::process::exit(1);
        }
        return;
    }
    let _ = (
        purgatory_common::version(),
        purgatory_simulation::version(),
        purgatory_protocol::version(),
    );
    println!(
        "PURGATORY server bootstrap OK {} protocol_version={}",
        purgatory_common::identity(),
        purgatory_protocol::PROTOCOL_VERSION
    );
    run_headless_clock_sample();
    run_headless_world_sample();
    if let Err(err) = network::run_blocking(network::ServerEndpointConfig::dev()) {
        let err = purgatory_persistence::redact_connection_text(&err);
        eprintln!("PURGATORY server error: {err}");
        std::process::exit(1);
    }
}

/// Headless local-development database administration. Ordinary startup never
/// calls this. Output is status text only; connection strings stay in the
/// environment.
fn database_admin_command() -> Option<Result<(), String>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = args.first().map(String::as_str)?;
    if !flag.starts_with("--database-") {
        return None;
    }
    let name = purgatory_persistence::LOCAL_DEV_DATABASE;
    match flag {
        "--database-status" => Some(database_runtime_status(name)),
        "--database-create" => {
            let request = match purgatory_persistence::DatabaseAdminRequest::from_env() {
                Ok(request) => request,
                Err(err) => {
                    return Some(Err(purgatory_persistence::redact_connection_text(
                        &err.to_string(),
                    )));
                }
            };
            let user = optional_flag(&args, "--user");
            Some(database_create(&request, name, user.as_deref()))
        }
        "--database-reset" => {
            let Some(confirm) = optional_flag(&args, "--confirm") else {
                return Some(Err("missing --confirm".into()));
            };
            let request = match purgatory_persistence::DatabaseAdminRequest::from_env() {
                Ok(request) => request,
                Err(err) => {
                    return Some(Err(purgatory_persistence::redact_connection_text(
                        &err.to_string(),
                    )));
                }
            };
            Some(database_reset(&request, name, &confirm))
        }
        "--database-add-user" => {
            let Some(user) = optional_flag(&args, "--user") else {
                return Some(Err("missing --user".into()));
            };
            let request = match purgatory_persistence::DatabaseAdminRequest::from_env_for_user_add()
            {
                Ok(request) => request,
                Err(err) => {
                    return Some(Err(purgatory_persistence::redact_connection_text(
                        &err.to_string(),
                    )));
                }
            };
            Some(database_add_user(&request, &user))
        }
        other => Some(Err(format!("unknown database command {other}"))),
    }
}

fn database_runtime_status(name: &str) -> Result<(), String> {
    let settings = purgatory_persistence::PostgresSettings::from_env()
        .map_err(|err| purgatory_persistence::redact_connection_text(&err.to_string()))?
        .ok_or_else(|| "PURGATORY_DATABASE_URL is required".to_string())?;
    let inspection = purgatory_persistence::inspect_runtime_database(&settings)
        .map_err(|err| purgatory_persistence::redact_connection_text(&err.to_string()))?;
    database_status_line(&inspection, name)
}

fn database_status_line(
    inspection: &purgatory_persistence::DatabaseInspection,
    name: &str,
) -> Result<(), String> {
    println!(
        "PURGATORY database status={} name={name} present={}",
        match inspection.status {
            purgatory_persistence::DatabaseStatus::Missing => "missing",
            purgatory_persistence::DatabaseStatus::Ready => "ready",
            purgatory_persistence::DatabaseStatus::Error => "error",
        },
        inspection.status != purgatory_persistence::DatabaseStatus::Missing
    );
    if !inspection.detail.is_empty() {
        let detail = purgatory_persistence::redact_connection_text(&inspection.detail);
        eprintln!("PURGATORY database detail: {detail}");
    }
    if inspection.status == purgatory_persistence::DatabaseStatus::Error {
        return Err(purgatory_persistence::redact_connection_text(
            &inspection.detail,
        ));
    }
    Ok(())
}

fn database_create(
    request: &purgatory_persistence::DatabaseAdminRequest,
    name: &str,
    user: Option<&str>,
) -> Result<(), String> {
    let login = user
        .map(purgatory_common::DevLogin::parse)
        .transpose()
        .map_err(|err| format!("{err:?}"))?;
    match purgatory_persistence::create_database(request, login.as_ref())
        .map_err(|err| err.to_string())?
    {
        purgatory_persistence::AdminOutcome::Created => {
            println!("PURGATORY database create=created name={name}");
        }
        purgatory_persistence::AdminOutcome::AlreadyInitialized => {
            println!("PURGATORY database create=already_initialized name={name}");
            if user.is_some() {
                println!("PURGATORY database user=not_added reason=already_initialized");
            }
        }
        purgatory_persistence::AdminOutcome::Reset => {
            return Err("create reported a reset".into());
        }
        purgatory_persistence::AdminOutcome::UserAdded
        | purgatory_persistence::AdminOutcome::UserAlreadyPresent => {
            return Err("create reported a user add".into());
        }
    }
    Ok(())
}

fn database_reset(
    request: &purgatory_persistence::DatabaseAdminRequest,
    name: &str,
    confirm: &str,
) -> Result<(), String> {
    match purgatory_persistence::reset_database(request, confirm).map_err(|err| err.to_string())? {
        purgatory_persistence::AdminOutcome::Reset => {
            println!("PURGATORY database reset=reset name={name}");
            Ok(())
        }
        other => Err(format!("reset finished as {other:?}")),
    }
}

fn database_add_user(
    request: &purgatory_persistence::DatabaseAdminRequest,
    user: &str,
) -> Result<(), String> {
    let login = purgatory_common::DevLogin::parse(user).map_err(|err| format!("{err:?}"))?;
    match purgatory_persistence::add_development_user(request, &login)
        .map_err(|err| err.to_string())?
    {
        purgatory_persistence::AdminOutcome::UserAdded
        | purgatory_persistence::AdminOutcome::UserAlreadyPresent => {
            println!("PURGATORY database user=added name={user}");
            Ok(())
        }
        other => Err(format!("add user finished as {other:?}")),
    }
}

fn optional_flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new("info,quinn=warn,quinn_proto=warn,rustls=warn")
    });
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

fn run_headless_clock_sample() {
    let mut clock = SimulationClock::new();
    let update = clock.advance(Duration::from_secs(1));
    println!(
        "PURGATORY server simulation clock OK ticks={} rate_hz={} discarded_ns={}",
        clock.tick().get(),
        TICK_RATE_HZ,
        update.discarded.as_nanos()
    );
}

fn run_headless_world_sample() {
    let mut world = World::footnote_test_stage();
    let entities = world.len();
    let player = world.player_id().is_some();
    let dt = Duration::from_nanos(purgatory_simulation::TICK_DURATION_NANOS).as_secs_f32();
    world.tick(dt, purgatory_simulation::PlayerInput::idle());
    let body = world.player_body();
    let grounded = body.map(|b| b.grounded).unwrap_or(false);
    println!(
        "PURGATORY server world OK entities={entities} player={player} after_tick={} grounded={grounded} footnote=ok",
        world.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_simulation::{Platform, TICK_DURATION_NANOS, Transform};
    use std::time::Duration;

    #[test]
    fn workspace_crates_are_linked() {
        assert!(!purgatory_common::version().is_empty());
        assert!(!purgatory_simulation::version().is_empty());
        assert!(!purgatory_protocol::version().is_empty());
        assert!(!purgatory_content::version().is_empty());
    }

    #[test]
    fn drives_simulation_clock_headlessly() {
        let mut clock = SimulationClock::new();
        let update = clock.advance(Duration::from_secs(1));
        assert_eq!(update.ticks_executed, 30);
        assert_eq!(update.discarded, Duration::ZERO);
        assert_eq!(clock.tick().get(), 30);
        assert_eq!(
            clock.simulation_time().as_nanos(),
            u128::from(TICK_DURATION_NANOS) * 30
        );
    }

    #[test]
    fn footnote_test_stage_is_headless() {
        let mut world = World::footnote_test_stage();
        assert!(world.len() >= 25);
        assert!(world.player_id().is_some());
        let dt = Duration::from_nanos(TICK_DURATION_NANOS).as_secs_f32();
        world.tick(dt, purgatory_simulation::PlayerInput::idle());
        assert!(world.player_body().expect("player").grounded);
    }

    #[test]
    fn world_lifecycle_is_headless() {
        let mut world = World::dev_stage();
        assert_eq!(world.len(), 5);
        assert!(world.player_id().is_some());
        let extra = world.spawn_platform(
            Transform::from_position([9.0, 0.0]),
            Platform::solid([0.2, 0.2]),
        );
        assert_eq!(world.len(), 6);
        assert!(world.despawn(extra));
        assert_eq!(world.len(), 5);
    }

    #[test]
    fn crate_manifest_excludes_client_ui_deps() {
        let manifest = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
        for forbidden in ["winit", "wgpu", "egui", "egui-winit", "egui-wgpu", "eframe"] {
            let present = manifest.lines().any(|line| {
                let trimmed = line.trim();
                trimmed.starts_with(forbidden) && (trimmed.contains('=') || trimmed.contains('{'))
            });
            assert!(
                !present,
                "{forbidden} must not appear as a purgatory-server dependency"
            );
        }
    }

    /// Startup failure (for example a port already in use) must be a concise
    /// message and a failure exit status, never a panic and never a readiness
    /// claim from the top level.
    #[test]
    fn bind_failure_is_a_clean_startup_failure() {
        let src = include_str!("main.rs");
        assert!(
            !src.contains(concat!("listen", "ing")),
            "only the network module may report readiness, and only after a real bind"
        );
        // Needles are split so this scan cannot match its own source.
        assert!(
            src.contains(concat!("eprintln!(\"PURGATORY server err", "or: {err}\")")),
            "startup failure must print one concise line"
        );
        assert!(
            src.contains(concat!("std::process::ex", "it(1)")),
            "startup failure must exit with a failure status"
        );
    }

    #[test]
    fn server_source_does_not_mention_egui() {
        let src = include_str!("main.rs");
        assert!(
            !src.contains(concat!("use e", "gui"))
                && !src.contains(concat!("extern crate e", "gui")),
            "headless server must not import egui"
        );
    }
}
