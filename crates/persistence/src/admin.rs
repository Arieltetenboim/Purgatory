//! Local development database administration.
//!
//! Server startup does not call this module. Developer Hub and the headless
//! `purgatory-server` administration flags do. Destructive actions are limited
//! to the configured local database or to an explicitly disposable test name.

use std::str::FromStr;

use ::postgres::Client;
use purgatory_common::DevLogin;

use crate::error::PersistError;
use crate::postgres::{self, PostgresSettings};

/// Physical database Developer Hub may create or reset.
pub const LOCAL_DEV_DATABASE: &str = "Purgatory_dev";
/// Deployment identity stored in that local database.
pub const LOCAL_DEV_DEPLOYMENT_ID: &str = "purgatory-dev";

const DISPOSABLE_PREFIX: &str = "p12a_db_";
const SYSTEM_DATABASES: &[&str] = &["postgres", "template0", "template1"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminAudience {
    /// The configured local development target. The database name is pinned.
    ConfiguredLocalDevelopment,
    /// A generated database for tests. Never `Purgatory_dev`.
    DisposableTest,
}

#[derive(Clone, Debug)]
pub struct DatabaseAdminRequest {
    pub maintenance_url: String,
    pub runtime_url: String,
    pub migration_url: String,
    pub schema: String,
    pub deployment_id: String,
    pub audience: AdminAudience,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DatabaseStatus {
    Missing,
    Ready,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatabaseInspection {
    pub status: DatabaseStatus,
    pub database: String,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminOutcome {
    Created,
    AlreadyInitialized,
    Reset,
    UserAdded,
    UserAlreadyPresent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminFailure {
    pub detail: String,
    /// `Some(false)` means the physical database is absent after the failure.
    pub database_present: Option<bool>,
}

impl std::fmt::Display for AdminFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for AdminFailure {}

impl DatabaseAdminRequest {
    /// Settings for the pinned local development database. A URL that names
    /// any other database is refused before a connection is opened.
    pub fn from_env() -> Result<Self, PersistError> {
        Self::from_env_with(true, |key| std::env::var(key), None)
    }

    /// Add-user uses the migration role. It does not read the administrator URL.
    pub fn from_env_for_user_add() -> Result<Self, PersistError> {
        Self::from_env_with(false, |key| std::env::var(key), None)
    }

    pub fn from_vars(
        getenv: impl FnMut(&str) -> Result<String, std::env::VarError>,
    ) -> Result<Self, PersistError> {
        Self::from_vars_inner(true, getenv)
    }

    pub(crate) fn from_env_with(
        require_admin: bool,
        mut getenv: impl FnMut(&str) -> Result<String, std::env::VarError>,
        file_root: Option<&std::path::Path>,
    ) -> Result<Self, PersistError> {
        let env_has_url = matches!(
            getenv("PURGATORY_DATABASE_URL"),
            Ok(value) if !value.trim().is_empty()
        );
        let file = if env_has_url {
            None
        } else if let Some(root) = file_root {
            crate::local_config::discover(root)
                .map_err(|err| PersistError::storage(err.to_string()))?
        } else {
            crate::local_config::discover_process_file()
                .map_err(|err| PersistError::storage(err.to_string()))?
        };
        Self::from_vars_inner(require_admin, |key| {
            lookup_setting(key, &mut getenv, file.as_ref())
        })
    }

    fn from_vars_inner(
        require_admin: bool,
        mut getenv: impl FnMut(&str) -> Result<String, std::env::VarError>,
    ) -> Result<Self, PersistError> {
        let runtime_url = required_var(&mut getenv, "PURGATORY_DATABASE_URL")?;
        let migration_url = required_var(&mut getenv, "PURGATORY_DATABASE_MIGRATION_URL")?;
        let maintenance_url = if require_admin {
            required_var(&mut getenv, "PURGATORY_DATABASE_ADMIN_URL")?
        } else {
            String::new()
        };
        let deployment_id = required_var(&mut getenv, "PURGATORY_DEPLOYMENT_ID")?;
        let schema = match getenv("PURGATORY_DATABASE_SCHEMA") {
            Ok(value) if !value.trim().is_empty() => value.trim().to_string(),
            _ => "purgatory_game".to_string(),
        };
        let request = Self {
            maintenance_url,
            runtime_url,
            migration_url,
            schema,
            deployment_id,
            audience: AdminAudience::ConfiguredLocalDevelopment,
        };
        if require_admin {
            request.validate()?;
        } else {
            request.validate_for_user_add()?;
        }
        Ok(request)
    }

    pub(crate) fn validate(&self) -> Result<ValidatedTarget, PersistError> {
        self.validate_inner(true)
    }

    pub(crate) fn validate_for_user_add(&self) -> Result<ValidatedTarget, PersistError> {
        self.validate_inner(false)
    }

    fn validate_inner(&self, require_admin: bool) -> Result<ValidatedTarget, PersistError> {
        postgres::validated_schema(&self.schema)?;
        postgres::validate_deployment_id(&self.deployment_id)?;
        let runtime = describe_url(&self.runtime_url, "runtime")?;
        let migration = describe_url(&self.migration_url, "migration")?;
        let maintenance = if require_admin {
            Some(describe_url(&self.maintenance_url, "administrator")?)
        } else {
            None
        };
        for endpoint in [&runtime, &migration] {
            if !endpoint.local {
                return Err(PersistError::storage(
                    "database administration is limited to a local PostgreSQL server",
                ));
            }
        }
        if let Some(endpoint) = &maintenance
            && !endpoint.local
        {
            return Err(PersistError::storage(
                "database administration is limited to a local PostgreSQL server",
            ));
        }
        if runtime.database != migration.database {
            return Err(PersistError::storage(
                "runtime and migration URLs must name the same database",
            ));
        }
        if let Some(maintenance) = &maintenance
            && maintenance.database != "postgres"
        {
            return Err(PersistError::storage(
                "administrator URL must connect to the postgres maintenance database",
            ));
        }
        if runtime.user.eq_ignore_ascii_case(&migration.user) {
            return Err(PersistError::storage(
                "administrator, migration, and runtime roles must be distinct",
            ));
        }
        if let Some(maintenance) = &maintenance
            && (runtime.user.eq_ignore_ascii_case(&maintenance.user)
                || migration.user.eq_ignore_ascii_case(&maintenance.user))
        {
            return Err(PersistError::storage(
                "administrator, migration, and runtime roles must be distinct",
            ));
        }
        if is_system_database(&runtime.database) {
            return Err(PersistError::storage(
                "refusing to administer a PostgreSQL system database",
            ));
        }
        match self.audience {
            AdminAudience::ConfiguredLocalDevelopment => {
                if runtime.database != LOCAL_DEV_DATABASE {
                    return Err(PersistError::storage(
                        "local development administration only allows the database Purgatory_dev",
                    ));
                }
                if self.deployment_id != LOCAL_DEV_DEPLOYMENT_ID {
                    return Err(PersistError::storage(
                        "local development deployment id must be purgatory-dev",
                    ));
                }
            }
            AdminAudience::DisposableTest => {
                if runtime.database.eq_ignore_ascii_case(LOCAL_DEV_DATABASE)
                    || !runtime.database.starts_with(DISPOSABLE_PREFIX)
                    || postgres::validated_schema(&runtime.database).is_err()
                {
                    return Err(PersistError::storage(
                        "disposable database administration requires a p12a_db_ name and refuses Purgatory_dev",
                    ));
                }
            }
        }
        Ok(ValidatedTarget {
            database: runtime.database,
            runtime_user: runtime.user,
            migration_user: migration.user,
        })
    }
}

struct Endpoint {
    database: String,
    user: String,
    local: bool,
}

#[derive(Debug)]
pub(crate) struct ValidatedTarget {
    database: String,
    runtime_user: String,
    migration_user: String,
}

fn required_var(
    getenv: &mut impl FnMut(&str) -> Result<String, std::env::VarError>,
    key: &str,
) -> Result<String, PersistError> {
    match getenv(key) {
        Ok(value) => {
            let value = value.trim();
            if value.is_empty() {
                Err(PersistError::storage(format!("{key} is required")))
            } else if value.contains("://") && key.ends_with("URL") {
                Ok(value.to_string())
            } else if value.contains("://") {
                Err(PersistError::storage(format!("{key} is not valid")))
            } else {
                Ok(value.to_string())
            }
        }
        Err(std::env::VarError::NotPresent) => {
            Err(PersistError::storage(format!("{key} is required")))
        }
        Err(_) => Err(PersistError::storage(format!("{key} is not valid unicode"))),
    }
}

fn describe_url(url: &str, label: &str) -> Result<Endpoint, PersistError> {
    let config = ::postgres::Config::from_str(url)
        .map_err(|_| PersistError::storage(format!("{label} database url is invalid")))?;
    let database = config
        .get_dbname()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            PersistError::storage(format!("{label} database url is missing a database name"))
        })?;
    let user = config
        .get_user()
        .filter(|user| !user.is_empty())
        .map(str::to_string)
        .ok_or_else(|| PersistError::storage(format!("{label} database url is missing a user")))?;
    let local = config.get_hosts().iter().all(|host| match host {
        ::postgres::config::Host::Tcp(name) => is_loopback(name),
        #[cfg(unix)]
        ::postgres::config::Host::Unix(_) => true,
    });
    Ok(Endpoint {
        database,
        user,
        local,
    })
}

fn is_loopback(host: &str) -> bool {
    let host = host.trim().trim_matches(['[', ']']);
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

fn is_system_database(name: &str) -> bool {
    SYSTEM_DATABASES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(name))
}

pub fn inspect(request: &DatabaseAdminRequest) -> Result<DatabaseInspection, AdminFailure> {
    let target = request.validate().map_err(fail_before_change)?;
    let mut maintenance = connect(&request.maintenance_url).map_err(fail_before_change)?;
    let present =
        database_exists(&mut maintenance, &target.database).map_err(fail_before_change)?;
    if !present {
        return Ok(DatabaseInspection {
            status: DatabaseStatus::Missing,
            database: target.database,
            detail: "database is absent".into(),
        });
    }
    match runtime_ready(request) {
        Ok(()) => Ok(DatabaseInspection {
            status: DatabaseStatus::Ready,
            database: target.database,
            detail: "initialized".into(),
        }),
        Err(err) => Ok(DatabaseInspection {
            status: DatabaseStatus::Error,
            database: target.database,
            detail: err.detail,
        }),
    }
}

pub fn create(
    request: &DatabaseAdminRequest,
    first_user: Option<&DevLogin>,
) -> Result<AdminOutcome, AdminFailure> {
    let target = request.validate().map_err(fail_before_change)?;
    if let Some(login) = first_user
        && login.as_str() == postgres::DEVELOPMENT_PROBE_LOGIN
    {
        return Err(fail_before_change(PersistError::storage(
            "the readiness probe is not a player account",
        )));
    }
    let mut maintenance = connect(&request.maintenance_url).map_err(fail_before_change)?;
    refuse_createdb(&mut maintenance, &target.runtime_user).map_err(fail_before_change)?;
    let present =
        database_exists(&mut maintenance, &target.database).map_err(fail_before_change)?;
    if present {
        return match runtime_ready(request) {
            Ok(()) => Ok(AdminOutcome::AlreadyInitialized),
            Err(err) => {
                if err.detail.contains("not bootstrapped")
                    || err
                        .detail
                        .contains("unsupported postgresql migration history")
                {
                    grant_database(&mut maintenance, &target).map_err(fail_before_change)?;
                    finish_initialization(request, &target, first_user)
                } else {
                    Err(err)
                }
            }
        };
    }
    let quoted = quote_ident(&mut maintenance, &target.database).map_err(fail_before_change)?;
    maintenance
        .batch_execute(&format!("CREATE DATABASE {quoted}"))
        .map_err(|err| fail_with_presence(err, Some(false)))?;
    grant_database(&mut maintenance, &target).map_err(|err| AdminFailure {
        detail: err.to_string(),
        database_present: Some(true),
    })?;
    match finish_initialization(request, &target, first_user) {
        Ok(outcome) => Ok(outcome),
        Err(err) => {
            let present = database_exists(&mut maintenance, &target.database).ok();
            Err(AdminFailure {
                detail: err.detail,
                database_present: present,
            })
        }
    }
}

pub fn reset(request: &DatabaseAdminRequest, confirm: &str) -> Result<AdminOutcome, AdminFailure> {
    let target = request.validate().map_err(fail_before_change)?;
    if confirm != target.database {
        return Err(fail_before_change(PersistError::storage(
            "reset confirmation does not match the configured database name",
        )));
    }
    let mut maintenance = connect(&request.maintenance_url).map_err(fail_before_change)?;
    refuse_createdb(&mut maintenance, &target.runtime_user).map_err(fail_before_change)?;
    let present =
        database_exists(&mut maintenance, &target.database).map_err(fail_before_change)?;
    if present {
        let sessions =
            other_sessions(&mut maintenance, &target.database).map_err(fail_before_change)?;
        if sessions > 0 {
            return Err(AdminFailure {
                detail: format!(
                    "refusing to reset {0}: {sessions} other database session(s) are still connected",
                    target.database
                ),
                database_present: Some(true),
            });
        }
        let quoted =
            quote_ident(&mut maintenance, &target.database).map_err(|err| AdminFailure {
                detail: err.to_string(),
                database_present: Some(true),
            })?;
        if let Err(err) = maintenance.batch_execute(&format!("DROP DATABASE {quoted}")) {
            let still = database_exists(&mut maintenance, &target.database).unwrap_or(true);
            return Err(AdminFailure {
                detail: crate::postgres::map_sql_pub(err).to_string(),
                database_present: Some(still),
            });
        }
    }
    #[cfg(test)]
    if FAIL_RECREATE_AFTER_DROP.swap(false, std::sync::atomic::Ordering::SeqCst) {
        return Err(AdminFailure {
            detail: format!(
                "reset deleted {} and recreation failed; the database is absent",
                target.database
            ),
            database_present: Some(false),
        });
    }
    let quoted = quote_ident(&mut maintenance, &target.database).map_err(|err| AdminFailure {
        detail: err.to_string(),
        database_present: Some(false),
    })?;
    if let Err(err) = maintenance.batch_execute(&format!("CREATE DATABASE {quoted}")) {
        return Err(AdminFailure {
            detail: format!(
                "reset deleted {} and recreation failed: {}",
                target.database,
                crate::postgres::map_sql_pub(err)
            ),
            database_present: Some(false),
        });
    }
    if let Err(err) = grant_database(&mut maintenance, &target) {
        return Err(AdminFailure {
            detail: format!(
                "reset deleted {} and recreation failed: {}",
                target.database, err
            ),
            database_present: Some(true),
        });
    }
    if let Err(err) = finish_initialization(request, &target, None) {
        let present = database_exists(&mut maintenance, &target.database).unwrap_or(true);
        return Err(AdminFailure {
            detail: format!(
                "reset deleted {} and initialization failed: {}",
                target.database, err.detail
            ),
            database_present: Some(present),
        });
    }
    Ok(AdminOutcome::Reset)
}

pub fn add_user(
    request: &DatabaseAdminRequest,
    login: &DevLogin,
) -> Result<AdminOutcome, AdminFailure> {
    let _target = request
        .validate_for_user_add()
        .map_err(fail_before_change)?;
    if login.as_str() == postgres::DEVELOPMENT_PROBE_LOGIN {
        return Err(fail_before_change(PersistError::storage(
            "the readiness probe is not a player account",
        )));
    }
    runtime_ready(request)?;
    let mut migration = connect(&request.migration_url).map_err(fail_before_change)?;
    postgres::prepare_schema(&mut migration, &request.schema, false).map_err(fail_before_change)?;
    let inserted = migration
        .execute(
            "INSERT INTO dev_users (login) VALUES ($1) ON CONFLICT DO NOTHING",
            &[&login.as_str()],
        )
        .map_err(|err| fail_before_change(crate::postgres::map_sql_pub(err)))?;
    if inserted == 1 {
        Ok(AdminOutcome::UserAdded)
    } else {
        Ok(AdminOutcome::UserAlreadyPresent)
    }
}

fn finish_initialization(
    request: &DatabaseAdminRequest,
    target: &ValidatedTarget,
    first_user: Option<&DevLogin>,
) -> Result<AdminOutcome, AdminFailure> {
    let mut migration = connect(&request.migration_url).map_err(fail_before_change)?;
    postgres::prepare_schema(&mut migration, &request.schema, true).map_err(fail_before_change)?;
    postgres::migrate(&mut migration).map_err(fail_before_change)?;
    postgres::grant_runtime(&mut migration, &request.schema, &target.runtime_user)
        .map_err(fail_before_change)?;
    match postgres::bootstrap_empty(&mut migration, &request.deployment_id) {
        Ok(()) => {
            if let Some(login) = first_user {
                migration
                    .execute(
                        "INSERT INTO dev_users (login) VALUES ($1)",
                        &[&login.as_str()],
                    )
                    .map_err(|err| fail_before_change(crate::postgres::map_sql_pub(err)))?;
            }
            runtime_ready(request)?;
            Ok(AdminOutcome::Created)
        }
        Err(err) if bootstrap_already_present(&err) => {
            runtime_ready(request)?;
            Ok(AdminOutcome::AlreadyInitialized)
        }
        Err(err) => Err(fail_before_change(err)),
    }
}

fn bootstrap_already_present(err: &PersistError) -> bool {
    matches!(err, PersistError::Conflict { reason, .. } if reason.contains("already bootstrapped"))
}

fn runtime_ready(request: &DatabaseAdminRequest) -> Result<(), AdminFailure> {
    let settings = PostgresSettings {
        url: request.runtime_url.clone(),
        migration_url: None,
        schema: request.schema.clone(),
        deployment_id: request.deployment_id.clone(),
    };
    PostgresSettings::open_check(&settings).map_err(fail_before_change)
}

fn grant_database(client: &mut Client, target: &ValidatedTarget) -> Result<(), PersistError> {
    let database = quote_ident(client, &target.database)?;
    let runtime = quote_ident(client, &target.runtime_user)?;
    let migration = quote_ident(client, &target.migration_user)?;
    client
        .batch_execute(&format!(
            "GRANT CONNECT ON DATABASE {database} TO {runtime}, {migration};
             GRANT CREATE ON DATABASE {database} TO {migration};"
        ))
        .map_err(crate::postgres::map_sql_pub)?;
    Ok(())
}

fn refuse_createdb(client: &mut Client, runtime_user: &str) -> Result<(), PersistError> {
    let row = client
        .query_opt(
            "SELECT rolcreatedb, rolsuper FROM pg_roles WHERE rolname = $1",
            &[&runtime_user],
        )
        .map_err(crate::postgres::map_sql_pub)?;
    let Some(row) = row else {
        return Err(PersistError::storage(
            "runtime role does not exist; refusing to administer the database",
        ));
    };
    let createdb: bool = row.get(0);
    let superuser: bool = row.get(1);
    if createdb || superuser {
        return Err(PersistError::storage(
            "runtime role must not have CREATE DATABASE or superuser privilege",
        ));
    }
    Ok(())
}

fn database_exists(client: &mut Client, name: &str) -> Result<bool, PersistError> {
    let exists: bool = client
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)",
            &[&name],
        )
        .map_err(crate::postgres::map_sql_pub)?
        .get(0);
    Ok(exists)
}

fn other_sessions(client: &mut Client, database: &str) -> Result<i64, PersistError> {
    let count: i64 = client
        .query_one(
            "SELECT count(*)::bigint FROM pg_stat_activity WHERE datname = $1 AND pid <> pg_backend_pid()",
            &[&database],
        )
        .map_err(crate::postgres::map_sql_pub)?
        .get(0);
    Ok(count)
}

fn quote_ident(client: &mut Client, name: &str) -> Result<String, PersistError> {
    let quoted: String = client
        .query_one("SELECT quote_ident($1)", &[&name])
        .map_err(crate::postgres::map_sql_pub)?
        .get(0);
    Ok(quoted)
}

fn connect(url: &str) -> Result<Client, PersistError> {
    postgres::connect_url(url)
}

fn fail_before_change(err: PersistError) -> AdminFailure {
    AdminFailure {
        detail: err.to_string(),
        database_present: None,
    }
}

fn lookup_setting(
    key: &str,
    getenv: &mut impl FnMut(&str) -> Result<String, std::env::VarError>,
    file: Option<&crate::local_config::LocalDatabaseConfig>,
) -> Result<String, std::env::VarError> {
    match getenv(key) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => file
            .and_then(|config| config.exported(key).map(str::to_string))
            .ok_or(std::env::VarError::NotPresent),
    }
}

/// Check the pinned local database with the runtime role. Does not connect as
/// the administrator and does not read `PURGATORY_DATABASE_ADMIN_URL`.
pub fn inspect_runtime(settings: &PostgresSettings) -> Result<DatabaseInspection, AdminFailure> {
    let endpoint = describe_url(&settings.url, "runtime").map_err(fail_before_change)?;
    if !endpoint.local {
        return Err(fail_before_change(PersistError::storage(
            "database administration is limited to a local PostgreSQL server",
        )));
    }
    if endpoint.database != LOCAL_DEV_DATABASE {
        return Err(fail_before_change(PersistError::storage(
            "local development administration only allows the database Purgatory_dev",
        )));
    }
    if settings.deployment_id != LOCAL_DEV_DEPLOYMENT_ID {
        return Err(fail_before_change(PersistError::storage(
            "local development deployment id must be purgatory-dev",
        )));
    }
    postgres::validated_schema(&settings.schema).map_err(fail_before_change)?;
    match PostgresSettings::open_check(settings) {
        Ok(()) => Ok(DatabaseInspection {
            status: DatabaseStatus::Ready,
            database: endpoint.database,
            detail: "initialized".into(),
        }),
        Err(err) => {
            let detail = crate::local_config::redact_connection_text(&err.to_string());
            if detail.to_ascii_lowercase().contains("does not exist") {
                Ok(DatabaseInspection {
                    status: DatabaseStatus::Missing,
                    database: LOCAL_DEV_DATABASE.to_string(),
                    detail: "database is absent".into(),
                })
            } else {
                Ok(DatabaseInspection {
                    status: DatabaseStatus::Error,
                    database: LOCAL_DEV_DATABASE.to_string(),
                    detail,
                })
            }
        }
    }
}

fn fail_with_presence(err: ::postgres::Error, present: Option<bool>) -> AdminFailure {
    AdminFailure {
        detail: crate::postgres::map_sql_pub(err).to_string(),
        database_present: present,
    }
}

#[cfg(test)]
static FAIL_RECREATE_AFTER_DROP: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(test)]
pub(crate) fn fail_next_reset_after_drop() {
    FAIL_RECREATE_AFTER_DROP.store(true, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_request(database: &str, host: &str, deployment: &str) -> DatabaseAdminRequest {
        DatabaseAdminRequest {
            maintenance_url: format!("postgres://admin:secret@{host}:5432/postgres"),
            runtime_url: format!("postgres://purgatory_dev:secret@{host}:5432/{database}"),
            migration_url: format!("postgres://purgatory_migrator:secret@{host}:5432/{database}"),
            schema: "purgatory_game".into(),
            deployment_id: deployment.into(),
            audience: AdminAudience::ConfiguredLocalDevelopment,
        }
    }

    #[test]
    fn runtime_inspection_does_not_use_an_administrator_url() {
        let settings = PostgresSettings {
            url: "postgresql://purgatory_dev:runtime-secret-value@db.example.com:5432/Purgatory_dev?sslmode=disable".into(),
            migration_url: None,
            schema: "purgatory_game".into(),
            deployment_id: LOCAL_DEV_DEPLOYMENT_ID.into(),
        };
        let err = inspect_runtime(&settings).unwrap_err().to_string();
        assert!(err.contains("local"), "{err}");
        assert!(!err.contains("runtime-secret-value"), "{err}");
        assert!(!err.contains("postgresql://"), "{err}");

        let wrong = PostgresSettings {
            url: "postgresql://purgatory_dev:runtime-secret-value@127.0.0.1:5432/other_dev?sslmode=disable".into(),
            migration_url: None,
            schema: "purgatory_game".into(),
            deployment_id: LOCAL_DEV_DEPLOYMENT_ID.into(),
        };
        let err = inspect_runtime(&wrong).unwrap_err().to_string();
        assert!(err.contains("Purgatory_dev"), "{err}");
        assert!(!err.contains("runtime-secret-value"), "{err}");
    }

    #[test]
    fn user_add_can_use_the_local_file_without_the_administrator_password() {
        let root = std::env::temp_dir().join(format!(
            "purgatory-admin-file-{}-{}",
            std::process::id(),
            unique_seq()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("config/local")).unwrap();
        std::fs::write(root.join("PHASE"), "12.12C\n").unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        std::fs::write(
            root.join("config/local/database.env"),
            "PURGATORY_DATABASE_URL=postgresql://purgatory_dev:runtime-secret-value@127.0.0.1:5432/Purgatory_dev?sslmode=disable\nPURGATORY_DATABASE_MIGRATION_URL=postgresql://purgatory_migrator:migration-secret-value@127.0.0.1:5432/Purgatory_dev?sslmode=disable\nPURGATORY_DATABASE_SCHEMA=purgatory_game\nPURGATORY_DEPLOYMENT_ID=purgatory-dev\n",
        )
        .unwrap();
        let added = DatabaseAdminRequest::from_env_with(
            false,
            |_| Err(std::env::VarError::NotPresent),
            Some(&root),
        )
        .unwrap();
        assert!(added.maintenance_url.is_empty());
        assert!(added.runtime_url.contains("/Purgatory_dev"));
        assert!(added.migration_url.contains("purgatory_migrator"));
        let missing_admin = DatabaseAdminRequest::from_env_with(
            true,
            |_| Err(std::env::VarError::NotPresent),
            Some(&root),
        )
        .unwrap_err()
        .to_string();
        assert!(
            missing_admin.contains("PURGATORY_DATABASE_ADMIN_URL"),
            "{missing_admin}"
        );
        assert!(
            !missing_admin.contains("runtime-secret-value"),
            "{missing_admin}"
        );
        assert!(
            !missing_admin.contains("migration-secret-value"),
            "{missing_admin}"
        );
        assert!(!missing_admin.contains("postgresql://"), "{missing_admin}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn local_create_accepts_only_the_pinned_database() {
        local_request(LOCAL_DEV_DATABASE, "127.0.0.1", LOCAL_DEV_DEPLOYMENT_ID)
            .validate()
            .unwrap();
    }

    #[test]
    fn reset_target_rejects_a_typo_remote_host_and_wrong_deployment() {
        let wrong_name = local_request("Purgatory_prod", "127.0.0.1", LOCAL_DEV_DEPLOYMENT_ID);
        assert!(wrong_name.validate().is_err());
        let remote = local_request(
            LOCAL_DEV_DATABASE,
            "db.example.com",
            LOCAL_DEV_DEPLOYMENT_ID,
        );
        assert!(remote.validate().unwrap_err().to_string().contains("local"));
        let wrong_id = local_request(LOCAL_DEV_DATABASE, "127.0.0.1", "production");
        assert!(wrong_id.validate().is_err());
        let system = local_request("postgres", "127.0.0.1", LOCAL_DEV_DEPLOYMENT_ID);
        assert!(system.validate().is_err());
    }

    #[test]
    fn disposable_tests_cannot_name_the_development_database() {
        let mut request = local_request("p12a_db_example", "localhost", "p12a-test");
        request.audience = AdminAudience::DisposableTest;
        request.validate().unwrap();
        let mut blocked = local_request(LOCAL_DEV_DATABASE, "127.0.0.1", "p12a-test");
        blocked.audience = AdminAudience::DisposableTest;
        assert!(blocked.validate().is_err());
    }

    #[test]
    fn confirmation_mismatch_does_not_claim_a_reset() {
        let request = local_request(LOCAL_DEV_DATABASE, "127.0.0.1", LOCAL_DEV_DEPLOYMENT_ID);
        let err = reset(&request, "purgatory_dev").unwrap_err();
        assert!(err.detail.contains("confirmation"));
        assert!(err.database_present.is_none());
    }

    #[test]
    #[ignore = "requires PURGATORY_TEST_DATABASE_URL and never touches Purgatory_dev"]
    fn disposable_database_create_reset_and_failed_recreate() {
        let admin_url = std::env::var("PURGATORY_TEST_DATABASE_URL").expect("test database url");
        assert!(
            !admin_url.to_ascii_lowercase().contains("purgatory_dev"),
            "refusing to administer Purgatory_dev from a test"
        );
        let database = format!("p12a_db_{}_{}", std::process::id(), unique_seq());
        let maintenance = set_database_name(&admin_url, "postgres");
        let password = format!("p12a{}", unique_seq());
        let migration_role = format!("p12a_m_{}", unique_seq());
        let runtime_role = format!("p12a_r_{}", unique_seq());
        let mut admin = connect(&maintenance).expect("maintenance connection");
        for role in [&migration_role, &runtime_role] {
            let sql: String = admin
                .query_one(
                    "SELECT format('CREATE ROLE %I LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT PASSWORD %L', $1::text, $2::text)",
                    &[role, &password],
                )
                .unwrap()
                .get(0);
            admin.batch_execute(&sql).unwrap();
        }
        let runtime_url = set_database_name(
            &replace_role(&admin_url, &runtime_role, &password),
            &database,
        );
        let migration_url = set_database_name(
            &replace_role(&admin_url, &migration_role, &password),
            &database,
        );
        let request = DatabaseAdminRequest {
            maintenance_url: maintenance.clone(),
            runtime_url: runtime_url.clone(),
            migration_url,
            schema: "p12a_admin".into(),
            deployment_id: "p12a-test".into(),
            audience: AdminAudience::DisposableTest,
        };
        let outcome = create(&request, Some(&login("dev.player"))).expect("create");
        assert_eq!(outcome, AdminOutcome::Created);
        let again = create(&request, Some(&login("dev.other"))).expect("second create");
        assert_eq!(again, AdminOutcome::AlreadyInitialized);
        let settings = crate::PostgresSettings::for_tests(runtime_url, "p12a_admin".into())
            .unwrap()
            .with_deployment_id("p12a-test".into())
            .unwrap();
        let mut service = crate::PersistenceService::open_postgresql(&settings).unwrap();
        assert!(service.user_registered(&login("dev.player")).unwrap());
        assert!(!service.user_registered(&login("dev.other")).unwrap());
        let entry = service
            .create_character(&login("dev.player"), "Hero")
            .unwrap();
        drop(service);

        let held = connect(&set_database_name(&admin_url, &database)).unwrap();
        let refused = reset(&request, &database).unwrap_err();
        assert!(refused.detail.contains("session"), "{}", refused.detail);
        drop(held);

        fail_next_reset_after_drop();
        let failed = reset(&request, &database).unwrap_err();
        assert_eq!(failed.database_present, Some(false), "{}", failed.detail);
        assert!(
            failed.detail.contains("recreation failed"),
            "{}",
            failed.detail
        );

        let restored = create(&request, Some(&login("dev.player"))).expect("recreate");
        assert_eq!(restored, AdminOutcome::Created);
        let mut service = crate::PersistenceService::open_postgresql(&settings).unwrap();
        assert!(service.roster(&login("dev.player")).unwrap().is_empty());
        assert!(
            !service
                .owns_character(&login("dev.player"), entry.character_id)
                .unwrap()
        );
        drop(service);

        reset(&request, &database).expect("final reset");
        let mut admin = connect(&maintenance).unwrap();
        let quoted = quote_ident(&mut admin, &database).unwrap();
        admin
            .batch_execute(&format!("DROP DATABASE {quoted}"))
            .unwrap();
        for role in [&migration_role, &runtime_role] {
            let sql: String = admin
                .query_one("SELECT format('DROP ROLE IF EXISTS %I', $1::text)", &[role])
                .unwrap()
                .get(0);
            admin.batch_execute(&sql).unwrap();
        }
    }

    fn unique_seq() -> u64 {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    fn login(name: &str) -> DevLogin {
        DevLogin::parse(name).unwrap()
    }

    fn set_database_name(url: &str, database: &str) -> String {
        let (base, query) = url.split_once('?').unwrap_or((url, ""));
        let slash = base.rfind('/').expect("database url path");
        let mut out = format!("{}/{}", &base[..slash], database);
        if !query.is_empty() {
            out.push('?');
            out.push_str(query);
        }
        out
    }

    fn replace_role(url: &str, user: &str, password: &str) -> String {
        let (scheme, rest) = url.split_once("://").unwrap();
        let host = rest.split_once('@').map(|(_, host)| host).unwrap_or(rest);
        format!("{scheme}://{user}:{password}@{host}")
    }
}
