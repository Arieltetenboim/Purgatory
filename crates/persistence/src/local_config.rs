//! Git-ignored local development database file.
//!
//! `config/local/database.env` supplies the runtime role, migration role,
//! schema, and deployment id. It does not store the administrator password.
//! Check and ordinary server startup use the runtime role only.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::admin::{LOCAL_DEV_DATABASE, LOCAL_DEV_DEPLOYMENT_ID};
use crate::postgres::{self, PostgresSettings};

pub const RELATIVE_PATH: &str = "config/local/database.env";

const RUNTIME_URL: &str = "PURGATORY_DATABASE_URL";
const MIGRATION_URL: &str = "PURGATORY_DATABASE_MIGRATION_URL";
const SCHEMA: &str = "PURGATORY_DATABASE_SCHEMA";
const DEPLOYMENT_ID: &str = "PURGATORY_DEPLOYMENT_ID";
const ADMIN_USER: &str = "PURGATORY_DATABASE_ADMIN_USER";
const ADMIN_URL: &str = "PURGATORY_DATABASE_ADMIN_URL";

/// Parsed local file. Passwords stay in memory for redaction and for building
/// child environment variables. This type has no `Debug` output.
pub struct LocalDatabaseConfig {
    runtime_url: String,
    migration_url: String,
    schema: String,
    deployment_id: String,
    admin_user: String,
    runtime_password: String,
    migration_password: String,
    host: String,
    port: u16,
}

impl std::fmt::Debug for LocalDatabaseConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalDatabaseConfig")
            .field("schema", &self.schema)
            .field("deployment_id", &self.deployment_id)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("runtime_url", &"[redacted]")
            .field("migration_url", &"[redacted]")
            .finish()
    }
}

#[derive(Debug)]
pub struct LocalDatabaseConfigError {
    message: String,
}

impl LocalDatabaseConfigError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for LocalDatabaseConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for LocalDatabaseConfigError {}

struct EndpointParts {
    user: String,
    password: String,
    host: String,
    port: u16,
    database: String,
}

impl LocalDatabaseConfig {
    pub fn load(path: &Path) -> Result<Self, LocalDatabaseConfigError> {
        let text = std::fs::read_to_string(path).map_err(|_| {
            LocalDatabaseConfigError::new("could not read config/local/database.env")
        })?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self, LocalDatabaseConfigError> {
        let values = parse_assignments(text)?;
        let runtime_url = required(&values, RUNTIME_URL)?;
        let migration_url = required(&values, MIGRATION_URL)?;
        let schema = required(&values, SCHEMA)?;
        let deployment_id = required(&values, DEPLOYMENT_ID)?;
        let admin_user = values
            .get(ADMIN_USER)
            .cloned()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "postgres".to_string());
        if deployment_id != LOCAL_DEV_DEPLOYMENT_ID {
            return Err(LocalDatabaseConfigError::new(
                "PURGATORY_DEPLOYMENT_ID must be purgatory-dev",
            ));
        }
        postgres::validated_schema(&schema).map_err(|_| {
            LocalDatabaseConfigError::new(
                "PURGATORY_DATABASE_SCHEMA must be a lowercase identifier",
            )
        })?;
        let runtime = endpoint(&runtime_url, RUNTIME_URL)?;
        let migration = endpoint(&migration_url, MIGRATION_URL)?;
        if runtime.database != LOCAL_DEV_DATABASE || migration.database != LOCAL_DEV_DATABASE {
            return Err(LocalDatabaseConfigError::new(
                "local database file must name the database Purgatory_dev",
            ));
        }
        if runtime.host != migration.host || runtime.port != migration.port {
            return Err(LocalDatabaseConfigError::new(
                "runtime and migration URLs must use the same local host and port",
            ));
        }
        if runtime.user.eq_ignore_ascii_case(&migration.user)
            || runtime.user.eq_ignore_ascii_case(&admin_user)
            || migration.user.eq_ignore_ascii_case(&admin_user)
        {
            return Err(LocalDatabaseConfigError::new(
                "administrator, migration, and runtime roles must be distinct",
            ));
        }
        if !is_role_name(&admin_user) {
            return Err(LocalDatabaseConfigError::new(
                "PURGATORY_DATABASE_ADMIN_USER must be a PostgreSQL role name",
            ));
        }
        Ok(Self {
            runtime_url,
            migration_url,
            schema,
            deployment_id,
            admin_user,
            runtime_password: runtime.password,
            migration_password: migration.password,
            host: runtime.host,
            port: runtime.port,
        })
    }

    /// Game-server settings. The migration password is not retained.
    pub fn runtime_settings(&self) -> PostgresSettings {
        PostgresSettings {
            url: self.runtime_url.clone(),
            migration_url: None,
            schema: self.schema.clone(),
            deployment_id: self.deployment_id.clone(),
        }
    }

    pub fn runtime_env(&self) -> Vec<(String, String)> {
        vec![
            (RUNTIME_URL.to_string(), self.runtime_url.clone()),
            (SCHEMA.to_string(), self.schema.clone()),
            (DEPLOYMENT_ID.to_string(), self.deployment_id.clone()),
        ]
    }

    pub fn migration_env(&self) -> Vec<(String, String)> {
        let mut env = self.runtime_env();
        env.push((MIGRATION_URL.to_string(), self.migration_url.clone()));
        env
    }

    /// Administrator URL for Create or Reset. The password is not stored in the file.
    pub fn admin_env(
        &self,
        admin_password: &str,
    ) -> Result<Vec<(String, String)>, LocalDatabaseConfigError> {
        if admin_password.is_empty() || admin_password.chars().any(char::is_control) {
            return Err(LocalDatabaseConfigError::new(
                "administrator password is required",
            ));
        }
        let admin_url = format!(
            "postgresql://{}:{}@{}:{}/postgres?sslmode=disable",
            encode_component(&self.admin_user),
            encode_component(admin_password),
            format_host(&self.host),
            self.port
        );
        endpoint(&admin_url, ADMIN_URL)?;
        let mut env = self.migration_env();
        env.push((ADMIN_URL.to_string(), admin_url));
        Ok(env)
    }

    pub fn exported(&self, key: &str) -> Option<&str> {
        match key {
            RUNTIME_URL => Some(self.runtime_url.as_str()),
            MIGRATION_URL => Some(self.migration_url.as_str()),
            SCHEMA => Some(self.schema.as_str()),
            DEPLOYMENT_ID => Some(self.deployment_id.as_str()),
            ADMIN_USER => Some(self.admin_user.as_str()),
            _ => None,
        }
    }

    pub fn redact(&self, text: &str) -> String {
        let mut text = redact_connection_text(text);
        let mut secrets = [
            self.runtime_password.as_str(),
            self.migration_password.as_str(),
        ];
        secrets.sort_by_key(|secret| std::cmp::Reverse(secret.len()));
        for secret in secrets {
            if secret.len() >= 4 {
                text = text.replace(secret, "[redacted]");
            }
        }
        text
    }
}

pub fn redact_connection_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        if let Some(length) = connection_url_len(&text[index..]) {
            out.push_str("[redacted]");
            index += length;
            continue;
        }
        let Some(ch) = text[index..].chars().next() else {
            break;
        };
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

/// Walk from `start` to a workspace root. A missing file is `Ok(None)`.
/// An unreadable or malformed file is an error and does not include secrets.
pub fn discover(start: &Path) -> Result<Option<LocalDatabaseConfig>, LocalDatabaseConfigError> {
    let Some(root) = workspace_root(start) else {
        return Ok(None);
    };
    let path = root.join(RELATIVE_PATH);
    if !path.is_file() {
        return Ok(None);
    }
    LocalDatabaseConfig::load(&path).map(Some)
}

pub fn discover_process_file() -> Result<Option<LocalDatabaseConfig>, LocalDatabaseConfigError> {
    let mut starts = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        starts.push(parent.to_path_buf());
    }
    for start in starts {
        match locate(&start)? {
            Locate::Found(config) => return Ok(Some(config)),
            Locate::WorkspaceWithoutFile => return Ok(None),
            Locate::NoWorkspace => continue,
        }
    }
    Ok(None)
}

enum Locate {
    Found(LocalDatabaseConfig),
    WorkspaceWithoutFile,
    NoWorkspace,
}

fn locate(start: &Path) -> Result<Locate, LocalDatabaseConfigError> {
    let Some(root) = workspace_root(start) else {
        return Ok(Locate::NoWorkspace);
    };
    let path = root.join(RELATIVE_PATH);
    if !path.is_file() {
        return Ok(Locate::WorkspaceWithoutFile);
    }
    Ok(Locate::Found(LocalDatabaseConfig::load(&path)?))
}

fn workspace_root(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        if dir.join("PHASE").is_file() && dir.join("Cargo.toml").is_file() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

fn parse_assignments(
    text: &str,
) -> Result<std::collections::BTreeMap<String, String>, LocalDatabaseConfigError> {
    let mut values = std::collections::BTreeMap::new();
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    for (number, raw) in text.lines().enumerate() {
        if raw.len() > 4096 {
            return Err(LocalDatabaseConfigError::new(format!(
                "config/local/database.env line {} is too long",
                number + 1
            )));
        }
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line).trim();
        let Some((key, value)) = line.split_once('=') else {
            return Err(LocalDatabaseConfigError::new(format!(
                "config/local/database.env line {} is not a KEY=VALUE setting",
                number + 1
            )));
        };
        let key = key.trim();
        if key == ADMIN_URL {
            return Err(LocalDatabaseConfigError::new(
                "PURGATORY_DATABASE_ADMIN_URL must not be stored in config/local/database.env. Enter the administrator password in Developer Hub for Create or Reset",
            ));
        }
        if !is_allowed_key(key) {
            return Err(LocalDatabaseConfigError::new(format!(
                "config/local/database.env has an unknown setting {key}"
            )));
        }
        if values.contains_key(key) {
            return Err(LocalDatabaseConfigError::new(format!(
                "config/local/database.env repeats {key}"
            )));
        }
        let value = parse_value(value);
        if value.chars().any(char::is_control) {
            return Err(LocalDatabaseConfigError::new(format!(
                "{key} contains a control character"
            )));
        }
        values.insert(key.to_string(), value);
    }
    Ok(values)
}

fn is_allowed_key(key: &str) -> bool {
    matches!(
        key,
        RUNTIME_URL | MIGRATION_URL | SCHEMA | DEPLOYMENT_ID | ADMIN_USER
    )
}

fn required(
    values: &std::collections::BTreeMap<String, String>,
    key: &str,
) -> Result<String, LocalDatabaseConfigError> {
    match values.get(key) {
        Some(value) if !value.is_empty() => Ok(value.clone()),
        _ => Err(LocalDatabaseConfigError::new(format!(
            "{key} is required in config/local/database.env"
        ))),
    }
}

fn parse_value(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        unescape(&trimmed[1..trimmed.len() - 1])
    } else {
        trimmed.to_string()
    }
}

fn unescape(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn endpoint(url: &str, label: &str) -> Result<EndpointParts, LocalDatabaseConfigError> {
    let config = ::postgres::Config::from_str(url)
        .map_err(|_| LocalDatabaseConfigError::new(format!("{label} is invalid")))?;
    let user = config
        .get_user()
        .filter(|user| !user.is_empty())
        .map(str::to_string)
        .ok_or_else(|| LocalDatabaseConfigError::new(format!("{label} is missing a user")))?;
    let password = config
        .get_password()
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .filter(|password| !password.is_empty())
        .ok_or_else(|| LocalDatabaseConfigError::new(format!("{label} is missing a password")))?;
    let database = config
        .get_dbname()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            LocalDatabaseConfigError::new(format!("{label} is missing a database name"))
        })?;
    let host = match config.get_hosts() {
        [::postgres::config::Host::Tcp(host)] if is_loopback(host) => host.clone(),
        _ => {
            return Err(LocalDatabaseConfigError::new(format!(
                "{label} must use one loopback host"
            )));
        }
    };
    let ports = config.get_ports();
    let port = match ports {
        [port] => *port,
        [] => 5432,
        _ => {
            return Err(LocalDatabaseConfigError::new(format!(
                "{label} must use one port"
            )));
        }
    };
    if !url.to_ascii_lowercase().contains("sslmode=disable")
        || url.to_ascii_lowercase().contains("sslmode=require")
        || url.to_ascii_lowercase().contains("sslmode=verify-")
        || url.to_ascii_lowercase().contains("sslmode=allow")
        || url.to_ascii_lowercase().contains("sslmode=prefer")
    {
        return Err(LocalDatabaseConfigError::new(format!(
            "{label} must set sslmode=disable for this loopback file"
        )));
    }
    Ok(EndpointParts {
        user,
        password,
        host,
        port,
        database,
    })
}

fn is_loopback(host: &str) -> bool {
    let host = host.trim().trim_matches(['[', ']']);
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

fn is_role_name(name: &str) -> bool {
    (1..=63).contains(&name.len())
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && name.starts_with(|ch: char| ch.is_ascii_alphabetic())
}

fn format_host(host: &str) -> String {
    if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

fn encode_component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn connection_url_len(text: &str) -> Option<usize> {
    const SCHEMES: &[&str] = &["postgresql://", "postgres://"];
    let lower = text.as_bytes();
    let scheme_len = SCHEMES.iter().find_map(|scheme| {
        let bytes = scheme.as_bytes();
        (lower.len() >= bytes.len() && lower[..bytes.len()].eq_ignore_ascii_case(bytes))
            .then_some(bytes.len())
    })?;
    let mut end = scheme_len;
    let rest = text.as_bytes();
    while end < rest.len()
        && !rest[end].is_ascii_whitespace()
        && rest[end] != b'"'
        && rest[end] != b'\''
    {
        end += 1;
    }
    Some(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"
# local development
PURGATORY_DATABASE_URL=postgresql://purgatory_dev:runtime-secret-value@127.0.0.1:5432/Purgatory_dev?sslmode=disable
PURGATORY_DATABASE_MIGRATION_URL=postgresql://purgatory_migrator:migration-secret-value@127.0.0.1:5432/Purgatory_dev?sslmode=disable
PURGATORY_DATABASE_SCHEMA=purgatory_game
PURGATORY_DEPLOYMENT_ID=purgatory-dev
PURGATORY_DATABASE_ADMIN_USER=postgres
"#;

    #[test]
    fn loads_runtime_migration_schema_and_deployment_without_an_admin_password() {
        let config = LocalDatabaseConfig::parse(FILE).unwrap();
        let runtime = config.runtime_env();
        assert!(runtime.iter().any(|(key, value)| {
            key == "PURGATORY_DATABASE_URL"
                && value.contains("purgatory_dev:")
                && value.contains("/Purgatory_dev")
        }));
        assert!(runtime.iter().any(|(key, value)| {
            key == "PURGATORY_DATABASE_SCHEMA" && value == "purgatory_game"
        }));
        assert!(
            runtime
                .iter()
                .any(|(key, value)| key == "PURGATORY_DEPLOYMENT_ID" && value == "purgatory-dev")
        );
        assert!(
            runtime
                .iter()
                .all(|(key, value)| key != "PURGATORY_DATABASE_ADMIN_URL"
                    && key != "PURGATORY_DATABASE_MIGRATION_URL"
                    && !value.contains("migration-secret-value"))
        );
        let settings = config.runtime_settings();
        assert!(settings.migration_url.is_none());
        assert!(settings.url.contains("/Purgatory_dev"));
        let migration = config.migration_env();
        assert!(migration.iter().any(|(key, value)| {
            key == "PURGATORY_DATABASE_MIGRATION_URL" && value.contains("purgatory_migrator:")
        }));
        let admin = config.admin_env("admin-secret-value").unwrap();
        let admin_url = admin
            .iter()
            .find(|(key, _)| key == "PURGATORY_DATABASE_ADMIN_URL")
            .map(|(_, value)| value.as_str())
            .unwrap();
        assert!(admin_url.starts_with("postgresql://postgres:"));
        assert!(admin_url.contains("/postgres?sslmode=disable"));
        assert!(admin_url.contains("admin-secret-value"));
        assert!(!admin_url.contains("runtime-secret-value"));
        assert!(!admin_url.contains("migration-secret-value"));
        assert!(config.admin_env("").is_err());
    }

    #[test]
    fn missing_and_malformed_settings_do_not_echo_secrets() {
        let missing = LocalDatabaseConfig::parse("PURGATORY_DATABASE_SCHEMA=purgatory_game\n");
        let err = missing.unwrap_err().to_string();
        assert!(err.contains("PURGATORY_DATABASE_URL is required"), "{err}");

        let secret = "super-secret-value";
        let malformed = format!(
            "PURGATORY_DATABASE_URL=not a url {secret}\nPURGATORY_DATABASE_MIGRATION_URL=postgresql://purgatory_migrator:{secret}@127.0.0.1:5432/Purgatory_dev?sslmode=disable\nPURGATORY_DATABASE_SCHEMA=purgatory_game\nPURGATORY_DEPLOYMENT_ID=purgatory-dev\n"
        );
        let err = LocalDatabaseConfig::parse(&malformed)
            .unwrap_err()
            .to_string();
        assert!(err.contains("PURGATORY_DATABASE_URL is invalid"), "{err}");
        assert!(!err.contains(secret), "{err}");
        assert!(!err.contains("postgresql://"), "{err}");

        let remote = FILE.replace("127.0.0.1", "db.example.com");
        let err = LocalDatabaseConfig::parse(&remote).unwrap_err().to_string();
        assert!(err.contains("loopback"), "{err}");
        assert!(!err.contains("runtime-secret-value"), "{err}");

        let wrong_db = FILE.replace("Purgatory_dev", "postgres");
        let err = LocalDatabaseConfig::parse(&wrong_db)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("Purgatory_dev") || err.contains("postgres"),
            "{err}"
        );
        assert!(!err.contains("runtime-secret-value"), "{err}");

        let stored_admin = format!(
            "{FILE}PURGATORY_DATABASE_ADMIN_URL=postgresql://postgres:admin-secret-value@127.0.0.1:5432/postgres?sslmode=disable\n"
        );
        let err = LocalDatabaseConfig::parse(&stored_admin)
            .unwrap_err()
            .to_string();
        assert!(err.contains("must not be stored"), "{err}");
        assert!(!err.contains("admin-secret-value"), "{err}");

        let duplicate = format!("{FILE}PURGATORY_DATABASE_SCHEMA=other_game\n");
        let err = LocalDatabaseConfig::parse(&duplicate)
            .unwrap_err()
            .to_string();
        assert!(err.contains("repeats"), "{err}");
        assert!(!err.contains("runtime-secret-value"), "{err}");
    }

    #[test]
    fn redacts_connection_urls_and_known_passwords() {
        let config = LocalDatabaseConfig::parse(FILE).unwrap();
        let leaked = "failed postgresql://purgatory_dev:runtime-secret-value@127.0.0.1:5432/Purgatory_dev?sslmode=disable password runtime-secret-value migration-secret-value".to_string();
        let clean = config.redact(&leaked);
        assert!(!clean.contains("runtime-secret-value"), "{clean}");
        assert!(!clean.contains("migration-secret-value"), "{clean}");
        assert!(!clean.contains("postgresql://"), "{clean}");
        assert!(clean.contains("[redacted]"), "{clean}");
        let generic = redact_connection_text(
            "postgres://postgres:admin-secret-value@127.0.0.1:5432/postgres?sslmode=disable",
        );
        assert_eq!(generic, "[redacted]");
    }

    #[test]
    fn discover_reads_only_the_workspace_file() {
        let root = std::env::temp_dir().join(format!(
            "purgatory-local-db-{}-{}",
            std::process::id(),
            unique()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("config/local")).unwrap();
        std::fs::write(root.join("PHASE"), "12.12C\n").unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        assert!(discover(&root).unwrap().is_none());
        std::fs::write(root.join(RELATIVE_PATH), FILE).unwrap();
        let config = discover(&root.join("config")).unwrap().unwrap();
        assert_eq!(config.runtime_settings().schema, "purgatory_game");
        std::fs::write(root.join(RELATIVE_PATH), "PURGATORY_DATABASE_URL=bad\n").unwrap();
        let err = discover(&root).unwrap_err().to_string();
        assert!(!err.contains("runtime-secret-value"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn explicit_environment_url_is_not_replaced_by_the_file() {
        let root = std::env::temp_dir().join(format!(
            "purgatory-local-env-{}-{}",
            std::process::id(),
            unique()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("config/local")).unwrap();
        std::fs::write(root.join("PHASE"), "12.12C\n").unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        std::fs::write(root.join(RELATIVE_PATH), FILE).unwrap();
        let explicit = "postgresql://purgatory_dev:other-secret-value@127.0.0.1:5432/Purgatory_dev?sslmode=disable";
        let settings = PostgresSettings::from_env_in(Some(&root), |key| match key {
            "PURGATORY_DATABASE_URL" => Ok(explicit.to_string()),
            "PURGATORY_DEPLOYMENT_ID" => Ok("purgatory-dev".to_string()),
            "PURGATORY_DATABASE_SCHEMA" => Ok("purgatory_game".to_string()),
            _ => Err(std::env::VarError::NotPresent),
        })
        .unwrap()
        .unwrap();
        assert!(settings.url.contains("other-secret-value"));
        assert!(!settings.url.contains("runtime-secret-value"));
        let from_file =
            PostgresSettings::from_env_in(Some(&root), |_| Err(std::env::VarError::NotPresent))
                .unwrap()
                .unwrap();
        assert!(from_file.url.contains("runtime-secret-value"));
        assert!(from_file.migration_url.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    fn unique() -> u64 {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
}
