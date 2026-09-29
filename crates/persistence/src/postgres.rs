//! Synchronous PostgreSQL writer used by the persistence worker.
//!
//! The client is blocking. Callers are the persistence worker thread, never the
//! 30 Hz simulation tick. One connection serves one service. Channels are not
//! databases and not schemas.

use std::collections::BTreeSet;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use postgres::types::ToSql;
use postgres::{Client, Config, NoTls};
use purgatory_common::{CharacterId, ContentId, DevLogin, ItemInstanceId};
use serde::Deserialize;
use serde::Serialize;

use crate::atomic::{read_committed_bytes, replace_file_recoverable};
use crate::character::{
    PERSISTENCE_SCHEMA_VERSION, PersistentCharacter, PersistentCharacterSnapshot,
};
use crate::domain::{
    self, CharacterItemLocation, CharacterNarrativeState, DurableCommand, DurableCommandResult,
    DurableContentRules, DurableEquipmentSlot, ItemOwner, ItemRecord, LiveDestination,
    NarrativeWrite, db_path,
};
use crate::error::PersistError;
use crate::identity::{self, CharacterRosterEntry, IDENTITY_FILE_NAME};
use crate::repository::character_file_name;

const MIGRATION_VERSION: i32 = 1;
const MIGRATION_BODY: &str = include_str!("../migrations/0001_foundation.sql");
const IMPORT_LOCK_KEY: i64 = 0x120A_0001;
pub(crate) const DURABLE_WRITER_FILE: &str = "durable_writer.json";
const RESERVED_DATABASES: &[&str] = &["purgatory_dev", "postgres", "template0", "template1"];

pub struct PostgresSettings {
    pub url: String,
    pub migration_url: Option<String>,
    pub schema: String,
}

impl PostgresSettings {
    pub fn from_env() -> Result<Option<Self>, PersistError> {
        let Ok(url) = std::env::var("PURGATORY_DATABASE_URL") else {
            return Ok(None);
        };
        let url = url.trim();
        if url.is_empty() {
            return Ok(None);
        }
        let migration_url = std::env::var("PURGATORY_DATABASE_MIGRATION_URL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let schema = std::env::var("PURGATORY_DATABASE_SCHEMA").unwrap_or_else(|_| "public".into());
        Ok(Some(Self {
            url: url.to_string(),
            migration_url,
            schema: schema.trim().to_string(),
        }))
    }

    /// Test connections must name a dedicated database and a disposable schema.
    /// `purgatory_dev` is refused before any connection is opened.
    pub fn for_tests(url: String, schema: String) -> Result<Self, PersistError> {
        let settings = Self {
            url,
            migration_url: None,
            schema,
        };
        settings.refuse_reserved_database()?;
        if settings.schema == "public" || !settings.schema.starts_with("p12a_") {
            return Err(PersistError::storage(
                "postgresql tests must use a disposable p12a_ schema",
            ));
        }
        validated_schema(&settings.schema)?;
        Ok(settings)
    }

    /// Runtime and migration URLs must name the same kind of dedicated database.
    pub fn with_migration_url(mut self, url: String) -> Result<Self, PersistError> {
        let migration = Self {
            url,
            migration_url: None,
            schema: self.schema.clone(),
        };
        migration.refuse_reserved_database()?;
        self.migration_url = Some(migration.url);
        Ok(self)
    }

    fn refuse_reserved_database(&self) -> Result<(), PersistError> {
        let name = database_name(&self.url)?;
        if RESERVED_DATABASES
            .iter()
            .any(|reserved| reserved.eq_ignore_ascii_case(&name))
        {
            return Err(PersistError::storage(
                "refusing a reserved database name; tests must use a dedicated database",
            ));
        }
        Ok(())
    }
}

pub(crate) struct PostgresStore {
    client: Client,
    rules: DurableContentRules,
    #[cfg(test)]
    hide_commit_reply: bool,
}

impl PostgresStore {
    pub(crate) fn open(dir: &Path, settings: &PostgresSettings) -> Result<Self, PersistError> {
        validated_schema(&settings.schema)?;
        if let Some(migration_url) = &settings.migration_url {
            let mut migrator = connect_url(migration_url)?;
            prepare_schema(&mut migrator, &settings.schema, true)?;
            migrate(&mut migrator)?;
            let runtime_user = database_user(&settings.url)?;
            let migration_user = database_user(migration_url)?;
            if runtime_user.eq_ignore_ascii_case(&migration_user) {
                return Err(PersistError::storage(
                    "migration role and runtime role must be distinct",
                ));
            }
            grant_runtime(&mut migrator, &settings.schema, &runtime_user)?;
            warn_durability(&mut migrator);
        }
        let mut client = connect_url(&settings.url)?;
        prepare_schema(
            &mut client,
            &settings.schema,
            settings.migration_url.is_none(),
        )?;
        if settings.migration_url.is_none() {
            migrate(&mut client)?;
        }
        warn_durability(&mut client);
        // Fence the file writer before the import transaction commits. A crash
        // in that interval leaves the marker and no cutover row, so the next
        // open imports the unchanged files instead of ignoring later file writes.
        fence_before_import(&mut client, dir)?;
        initialize(&mut client, dir)?;
        write_marker(dir)?;
        Ok(Self {
            client,
            rules: DurableContentRules::new(),
            #[cfg(test)]
            hide_commit_reply: false,
        })
    }

    pub(crate) fn set_rules(&mut self, rules: DurableContentRules) {
        self.rules = rules;
    }

    pub(crate) fn commit(
        &mut self,
        command: &DurableCommand,
    ) -> Result<DurableCommandResult, PersistError> {
        let request = canonical_request(command)?;
        // A stored result is returned before content rules are applied again.
        // Retry after a catalog change must still answer the original commit.
        if let Some(result) = stored_result_if_same(&mut self.client, &command.key, &request)? {
            return Ok(result);
        }
        domain::validate_command(command, &self.rules)?;
        match commit_once(&mut self.client, command, &request, &self.rules) {
            Ok(result) => {
                if self.consume_hidden_reply() {
                    return recover_hidden_commit(&mut self.client, &command.key, &request);
                }
                Ok(result)
            }
            Err(err) if command_key_race(&err) || commit_reply_lost(&err) => {
                match stored_result_if_same(&mut self.client, &command.key, &request) {
                    Ok(Some(result)) => Ok(result),
                    Ok(None) if commit_reply_lost(&err) => Err(PersistError::storage(
                        "commit outcome unknown: the command key was not committed and the connection could not prove it",
                    )),
                    Ok(None) => Err(err),
                    Err(read_err) => Err(PersistError::storage(format!(
                        "commit outcome unknown: {read_err}"
                    ))),
                }
            }
            Err(err) => Err(err),
        }
    }

    fn consume_hidden_reply(&mut self) -> bool {
        #[cfg(test)]
        {
            let hide = self.hide_commit_reply;
            self.hide_commit_reply = false;
            hide
        }
        #[cfg(not(test))]
        {
            false
        }
    }

    #[cfg(test)]
    pub(crate) fn hide_next_commit_reply(&mut self) {
        self.hide_commit_reply = true;
    }

    pub(crate) fn save_restore(
        &mut self,
        snapshot: PersistentCharacterSnapshot,
    ) -> Result<(), PersistError> {
        save_restore(&mut self.client, snapshot)
    }

    pub(crate) fn create_character(
        &mut self,
        login: &DevLogin,
        name: &str,
    ) -> Result<CharacterRosterEntry, PersistError> {
        create_character(&mut self.client, login, name)
    }

    pub(crate) fn resolve_or_create(
        &mut self,
        login: &DevLogin,
    ) -> Result<PersistentCharacter, PersistError> {
        if let Some(id) = lookup(&mut self.client, login)? {
            return load_character(&mut self.client, id);
        }
        let used = name_keys(&mut self.client)?;
        for ordinal in 0..=used.len() as u64 {
            let name = identity::compatibility_name(ordinal)
                .ok_or(PersistError::CompatibilityNamesExhausted)?;
            if !used.contains(&name.uniqueness_key()) {
                let entry = create_character(&mut self.client, login, name.as_str())?;
                return load_character(&mut self.client, entry.character_id);
            }
        }
        Err(PersistError::CompatibilityNamesExhausted)
    }

    pub(crate) fn lookup(&mut self, login: &DevLogin) -> Result<Option<CharacterId>, PersistError> {
        lookup(&mut self.client, login)
    }

    pub(crate) fn roster(
        &mut self,
        login: &DevLogin,
    ) -> Result<Vec<CharacterRosterEntry>, PersistError> {
        roster(&mut self.client, login)
    }

    pub(crate) fn owns(&mut self, login: &DevLogin, id: CharacterId) -> Result<bool, PersistError> {
        owns(&mut self.client, login, id)
    }

    pub(crate) fn load_character(
        &mut self,
        id: CharacterId,
    ) -> Result<PersistentCharacter, PersistError> {
        load_character(&mut self.client, id)
    }

    pub(crate) fn item(&mut self, id: ItemInstanceId) -> Result<Option<ItemRecord>, PersistError> {
        load_item(&mut self.client, id)
    }

    pub(crate) fn narrative(
        &mut self,
        id: CharacterId,
    ) -> Result<CharacterNarrativeState, PersistError> {
        load_narrative(&mut self.client, id)
    }
}

pub(crate) fn reject_file_writer_if_cut_over(dir: &Path) -> Result<(), PersistError> {
    let path = dir.join(DURABLE_WRITER_FILE);
    let Some(bytes) = read_source(&path)? else {
        return Ok(());
    };
    let marker: WriterMarker = serde_json::from_slice(&bytes)
        .map_err(|err| PersistError::corrupt(&path, format!("durable writer marker: {err}")))?;
    if marker.schema_version == 1 && marker.writer == "postgresql" {
        return Err(PersistError::migration(
            &path,
            "postgresql is the durable writer; set PURGATORY_DATABASE_URL",
        ));
    }
    Err(PersistError::corrupt(
        &path,
        "unrecognized durable writer marker",
    ))
}

#[cfg(test)]
pub(crate) fn count_table(settings: &PostgresSettings, table: &str) -> Result<i64, PersistError> {
    match table {
        "dev_users"
        | "characters"
        | "item_instances"
        | "durable_commands"
        | "character_facts"
        | "character_learned_abilities" => {}
        _ => return Err(PersistError::storage("unsupported postgresql test count")),
    }
    let mut client = connect_url(&settings.url)?;
    prepare_schema(&mut client, &settings.schema, false)?;
    let sql = format!("SELECT count(*)::bigint FROM {table}");
    let row = client.query_one(&sql, &[]).map_err(map_sql)?;
    Ok(row.get(0))
}

#[cfg(test)]
pub(crate) fn drop_test_schema(settings: &PostgresSettings) -> Result<(), PersistError> {
    if !settings.schema.starts_with("p12a_") {
        return Err(PersistError::storage(
            "refusing to drop a schema that is not a disposable p12a_ test schema",
        ));
    }
    validated_schema(&settings.schema)?;
    let mut client = connect_url(&settings.url)?;
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {} CASCADE",
            settings.schema
        ))
        .map_err(map_sql)?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct WriterMarker {
    schema_version: u32,
    writer: String,
}

fn write_marker(dir: &Path) -> Result<(), PersistError> {
    let path = dir.join(DURABLE_WRITER_FILE);
    #[cfg(test)]
    if MARKER_WRITE_FAILS.swap(false, Ordering::Relaxed) {
        return Err(PersistError::io(
            &path,
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "simulated durable writer marker failure",
            ),
        ));
    }
    let bytes = br#"{"schema_version":1,"writer":"postgresql"}"#;
    replace_file_recoverable(&path, bytes).map_err(|err| PersistError::io(&path, err))
}

#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
static MARKER_WRITE_FAILS: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
pub(crate) fn fail_next_marker_write() {
    MARKER_WRITE_FAILS.store(true, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn fence_file_writer(dir: &Path) -> Result<(), PersistError> {
    write_marker(dir)
}

fn connect_url(url: &str) -> Result<Client, PersistError> {
    let mut config =
        Config::from_str(url).map_err(|_| PersistError::storage("invalid database url"))?;
    let name = config.get_dbname().unwrap_or("");
    if name.is_empty() {
        return Err(PersistError::storage(
            "database url is missing a database name",
        ));
    }
    config.application_name("purgatory-persistence");
    if config.get_connect_timeout().is_none() {
        config.connect_timeout(Duration::from_secs(10));
    }
    if tls_required(url) {
        let connector = native_tls::TlsConnector::builder()
            .build()
            .map_err(|_| PersistError::storage("tls connector failed"))?;
        let tls = postgres_native_tls::MakeTlsConnector::new(connector);
        config.connect(tls).map_err(map_sql)
    } else {
        config.connect(NoTls).map_err(map_sql)
    }
}

fn tls_required(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    [
        "sslmode=require",
        "sslmode=verify-ca",
        "sslmode=verify-full",
    ]
    .iter()
    .any(|mode| lower.contains(mode))
}

fn database_name(url: &str) -> Result<String, PersistError> {
    let config =
        Config::from_str(url).map_err(|_| PersistError::storage("invalid database url"))?;
    config
        .get_dbname()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .ok_or_else(|| PersistError::storage("database url is missing a database name"))
}

fn database_user(url: &str) -> Result<String, PersistError> {
    let config =
        Config::from_str(url).map_err(|_| PersistError::storage("invalid database url"))?;
    config
        .get_user()
        .filter(|user| !user.is_empty())
        .map(str::to_string)
        .ok_or_else(|| PersistError::storage("database url is missing a user"))
}

fn grant_runtime(
    client: &mut Client,
    schema: &str,
    runtime_user: &str,
) -> Result<(), PersistError> {
    let schema_sql: String = client
        .query_one("SELECT quote_ident($1)", &[&schema])
        .map_err(map_sql)?
        .get(0);
    let user_sql: String = client
        .query_one("SELECT quote_ident($1)", &[&runtime_user])
        .map_err(map_sql)?
        .get(0);
    // Runtime can read the migration history and change character-owned rows.
    // It cannot create or drop objects. Fact clears are the only deletes.
    // Command retry locks the stored row with SELECT FOR UPDATE, which
    // requires UPDATE on durable_commands even though the row is not rewritten.
    let sql = format!(
        "GRANT USAGE ON SCHEMA {schema_sql} TO {user_sql};
         GRANT SELECT ON {schema_sql}.schema_migrations TO {user_sql};
         GRANT SELECT, INSERT, UPDATE ON {schema_sql}.durable_meta, {schema_sql}.characters, {schema_sql}.item_instances TO {user_sql};
         GRANT SELECT, INSERT ON {schema_sql}.dev_users, {schema_sql}.character_npcs_met, {schema_sql}.character_dialogue_heard, {schema_sql}.character_learned_abilities TO {user_sql};
         GRANT SELECT, INSERT, UPDATE ON {schema_sql}.durable_commands TO {user_sql};
         GRANT SELECT, INSERT, UPDATE, DELETE ON {schema_sql}.character_facts TO {user_sql};"
    );
    client.batch_execute(&sql).map_err(map_sql)
}

fn validated_schema(name: &str) -> Result<(), PersistError> {
    let ok = (1..=63).contains(&name.len())
        && name.starts_with(|ch: char| ch.is_ascii_lowercase())
        && name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_');
    if ok {
        Ok(())
    } else {
        Err(PersistError::storage(
            "postgresql schema name must be a lowercase identifier",
        ))
    }
}

fn prepare_schema(client: &mut Client, schema: &str, create: bool) -> Result<(), PersistError> {
    validated_schema(schema)?;
    if create {
        client
            .batch_execute(&format!("CREATE SCHEMA IF NOT EXISTS {schema}"))
            .map_err(map_sql)?;
    }
    client
        .batch_execute(&format!("SET search_path TO {schema}"))
        .map_err(map_sql)?;
    Ok(())
}

fn migrate(client: &mut Client) -> Result<(), PersistError> {
    let present: bool = client
        .query_one("SELECT to_regclass('schema_migrations') IS NOT NULL", &[])
        .map_err(map_sql)?
        .get(0);
    if !present {
        let mut tx = client.transaction().map_err(map_sql)?;
        tx.batch_execute(MIGRATION_BODY).map_err(map_sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, body) VALUES ($1, $2)",
            &[&MIGRATION_VERSION, &MIGRATION_BODY],
        )
        .map_err(map_sql)?;
        return tx.commit().map_err(map_sql);
    }
    let rows = client
        .query(
            "SELECT version, body FROM schema_migrations ORDER BY version",
            &[],
        )
        .map_err(map_sql)?;
    if rows.len() != 1 {
        return Err(PersistError::migration(
            db_path(),
            "unexpected postgresql migration history",
        ));
    }
    let version: i32 = rows[0].get(0);
    let body: String = rows[0].get(1);
    if version != MIGRATION_VERSION || body != MIGRATION_BODY {
        return Err(PersistError::migration(
            db_path(),
            "applied postgresql migration does not match this build",
        ));
    }
    Ok(())
}

fn warn_durability(client: &mut Client) {
    for name in ["fsync", "synchronous_commit", "full_page_writes"] {
        let Ok(row) = client.query_one("SELECT current_setting($1)", &[&name]) else {
            continue;
        };
        let value: String = row.get(0);
        let insufficient = match name {
            "synchronous_commit" => value == "off",
            _ => value == "off",
        };
        if insufficient {
            eprintln!(
                "PURGATORY postgresql durability setting {name}={value}; acknowledged character commits require fsync, synchronous_commit, and full_page_writes on before production"
            );
        }
    }
}

fn initialize(client: &mut Client, dir: &Path) -> Result<(), PersistError> {
    let mut tx = client.transaction().map_err(map_sql)?;
    let result = (|| {
        tx.execute("SELECT pg_advisory_xact_lock($1)", &[&IMPORT_LOCK_KEY])
            .map_err(map_sql)?;
        let existing = tx
            .query_opt("SELECT value FROM durable_meta WHERE key = 'cutover'", &[])
            .map_err(map_sql)?;
        if existing.is_some() {
            return Ok(());
        }
        if !marker_is_postgresql(dir)? {
            return Err(PersistError::migration(
                dir,
                "refusing to import before the file writer is fenced",
            ));
        }
        let plan = inventory_source(dir)?;
        insert_import(&mut tx, &plan)?;
        Ok(())
    })();
    match result {
        Ok(()) => tx.commit().map_err(|err| ambiguous_commit(map_sql(err))),
        Err(err) => {
            let _ = tx.rollback();
            Err(err)
        }
    }
}

fn fence_before_import(client: &mut Client, dir: &Path) -> Result<(), PersistError> {
    let cutover = cutover_present(client)?;
    let marker = marker_is_postgresql(dir)?;
    if cutover && !marker {
        return Err(PersistError::migration(
            dir,
            "database cutover is committed but durable_writer.json is missing; refusing to open so file changes are not ignored",
        ));
    }
    if !cutover {
        write_marker(dir)?;
    }
    Ok(())
}

fn cutover_present(client: &mut Client) -> Result<bool, PersistError> {
    let present: bool = client
        .query_one("SELECT to_regclass('durable_meta') IS NOT NULL", &[])
        .map_err(map_sql)?
        .get(0);
    if !present {
        return Ok(false);
    }
    let row = client
        .query_opt("SELECT value FROM durable_meta WHERE key = 'cutover'", &[])
        .map_err(map_sql)?;
    Ok(row.is_some())
}

fn marker_is_postgresql(dir: &Path) -> Result<bool, PersistError> {
    let path = dir.join(DURABLE_WRITER_FILE);
    let Some(bytes) = read_source(&path)? else {
        return Ok(false);
    };
    let marker: WriterMarker = serde_json::from_slice(&bytes)
        .map_err(|err| PersistError::corrupt(&path, format!("durable writer marker: {err}")))?;
    Ok(marker.schema_version == 1 && marker.writer == "postgresql")
}

struct ImportPlan {
    next_character_id: u64,
    rows: Vec<ImportRow>,
    cutover: &'static str,
}

struct ImportRow {
    login: String,
    entry: CharacterRosterEntry,
    position: i32,
    character: PersistentCharacter,
}

fn inventory_source(dir: &Path) -> Result<ImportPlan, PersistError> {
    if !dir.exists() {
        return Ok(empty_plan());
    }
    let identity = identity::read_identity_source(dir)?;
    let mut file_ids = BTreeSet::new();
    for entry in std::fs::read_dir(dir).map_err(|err| PersistError::io(dir, err))? {
        let entry = entry.map_err(|err| PersistError::io(dir, err))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(PersistError::corrupt(
                dir,
                "persistence directory has a non-utf8 file name",
            ));
        };
        if is_allowed_sidecar(name) {
            if let Some(id) = character_id_in_filename(name) {
                file_ids.insert(id);
            }
            continue;
        }
        return Err(PersistError::corrupt(
            dir,
            format!("unexpected persistence file {name}"),
        ));
    }
    let Some(identity) = identity else {
        if !file_ids.is_empty() {
            return Err(PersistError::corrupt(
                dir,
                "character files exist without an identity roster",
            ));
        }
        return Ok(empty_plan());
    };
    let mut rows = Vec::new();
    let mut roster_ids = BTreeSet::new();
    for (login, roster) in &identity.logins {
        for (position, entry) in roster.iter().enumerate() {
            roster_ids.insert(entry.character_id);
            let character = load_import_character(dir, entry.character_id)?;
            rows.push(ImportRow {
                login: login.clone(),
                entry: entry.clone(),
                position: i32::try_from(position)
                    .map_err(|_| PersistError::corrupt(dir, "roster position does not fit"))?,
                character,
            });
        }
    }
    for id in &file_ids {
        if !roster_ids.contains(id) {
            return Err(PersistError::corrupt(
                dir,
                format!("character file {} is not in the identity roster", id.raw()),
            ));
        }
    }
    Ok(ImportPlan {
        next_character_id: identity.next_character_id,
        rows,
        cutover: "imported",
    })
}

fn empty_plan() -> ImportPlan {
    ImportPlan {
        next_character_id: 1,
        rows: Vec::new(),
        cutover: "fresh",
    }
}

fn is_allowed_sidecar(name: &str) -> bool {
    name == IDENTITY_FILE_NAME
        || name == DURABLE_WRITER_FILE
        || name == "identity.json.bak"
        || name == "identity.json.tmp"
        || name == "durable_writer.json.bak"
        || name == "durable_writer.json.tmp"
        || character_id_in_filename(name).is_some()
}

fn character_id_in_filename(name: &str) -> Option<CharacterId> {
    let base = name
        .strip_suffix(".tmp")
        .or_else(|| name.strip_suffix(".bak"))
        .unwrap_or(name);
    let hex = base.strip_prefix("char_")?.strip_suffix(".json")?;
    if hex.len() != 16
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return None;
    }
    let raw = u64::from_str_radix(hex, 16).ok()?;
    Some(CharacterId::from_raw(raw))
}

fn load_import_character(dir: &Path, id: CharacterId) -> Result<PersistentCharacter, PersistError> {
    let path = dir.join(character_file_name(id));
    let Some(bytes) = read_source(&path)? else {
        let mut character = PersistentCharacter::new_default(id);
        character.schema_version = PERSISTENCE_SCHEMA_VERSION;
        return Ok(character);
    };
    let parsed: PersistentCharacter =
        serde_json::from_slice(&bytes).map_err(|err| PersistError::json(&path, err))?;
    if parsed.schema_version != PERSISTENCE_SCHEMA_VERSION {
        return Err(PersistError::schema(&path, parsed.schema_version));
    }
    if parsed.character_id != id {
        return Err(PersistError::corrupt(
            &path,
            format!(
                "file character_id {} does not match {}",
                parsed.character_id, id
            ),
        ));
    }
    parsed.validate(&path)?;
    Ok(parsed)
}

fn read_source(path: &Path) -> Result<Option<Vec<u8>>, PersistError> {
    read_committed_bytes(path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::InvalidData {
            PersistError::corrupt(path, err.to_string())
        } else {
            PersistError::io(path, err)
        }
    })
}

fn insert_import(
    tx: &mut postgres::Transaction<'_>,
    plan: &ImportPlan,
) -> Result<(), PersistError> {
    let next_character = plan.next_character_id.to_string();
    tx.execute(
        "INSERT INTO durable_meta (key, value) VALUES ('next_character_id', $1), ('next_item_instance_id', '1'), ('cutover', $2)",
        &[&next_character, &plan.cutover],
    )
    .map_err(map_sql)?;
    for row in &plan.rows {
        tx.execute(
            "INSERT INTO dev_users (login) VALUES ($1) ON CONFLICT DO NOTHING",
            &[&row.login.as_str()],
        )
        .map_err(map_sql)?;
        insert_character_row(tx, &row.login, row.position, &row.character, &row.entry)?;
    }
    Ok(())
}

fn insert_character_row(
    tx: &mut postgres::Transaction<'_>,
    login: &str,
    position: i32,
    character: &PersistentCharacter,
    entry: &CharacterRosterEntry,
) -> Result<(), PersistError> {
    let id = id_bytes(character.character_id.raw());
    let revision = revision_i64(character.persistence_revision)?;
    let checkpoint = character.restore.checkpoint_id.as_deref();
    let exit_reason = character
        .instance_exit
        .as_ref()
        .and_then(|exit| exit.reason.as_deref());
    let name_key = entry.display_name.uniqueness_key();
    tx.execute(
        "INSERT INTO characters (
            character_id, owner_login, display_name, name_key, roster_position,
            persistence_revision, restore_map_authored, restore_point_id,
            restore_checkpoint_id, instance_exit_reason
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        &[
            &id.as_slice() as &(dyn ToSql + Sync),
            &login,
            &entry.display_name.as_str(),
            &name_key.as_str(),
            &position,
            &revision,
            &character.restore.map_authored.as_str(),
            &character.restore.point_id.as_str(),
            &checkpoint,
            &exit_reason,
        ],
    )
    .map_err(map_sql)?;
    Ok(())
}

fn commit_once(
    client: &mut Client,
    command: &DurableCommand,
    request: &str,
    rules: &DurableContentRules,
) -> Result<DurableCommandResult, PersistError> {
    let mut tx = client.transaction().map_err(map_sql)?;
    let result = apply_command(&mut tx, command, request, rules);
    match result {
        Ok(value) => match tx.commit() {
            Ok(()) => Ok(value),
            Err(err) => Err(ambiguous_commit(map_sql(err))),
        },
        Err(err) => {
            let _ = tx.rollback();
            Err(err)
        }
    }
}

fn apply_command(
    tx: &mut postgres::Transaction<'_>,
    command: &DurableCommand,
    request: &str,
    rules: &DurableContentRules,
) -> Result<DurableCommandResult, PersistError> {
    if let Some(result) = locked_command(tx, &command.key, request)? {
        return Ok(result);
    }
    tx.execute(
        "SELECT value FROM durable_meta WHERE key = 'next_item_instance_id' FOR UPDATE",
        &[],
    )
    .map_err(map_sql)?;
    let mut locked = BTreeSet::new();
    let mut expected_ids: Vec<_> = command
        .expected_revisions
        .iter()
        .map(|(id, revision)| (*id, *revision))
        .collect();
    expected_ids.sort_by_key(|(id, _)| id.raw());
    for (id, expected) in &expected_ids {
        let current = lock_revision(tx, *id)?;
        if current != *expected {
            return Err(PersistError::conflict(
                db_path(),
                format!(
                    "character {} revision is {current}, command expected {expected}",
                    id.raw()
                ),
            ));
        }
        locked.insert(*id);
    }
    let mut item_ids: Vec<_> = command
        .moves
        .iter()
        .map(|item| item.item_instance_id)
        .chain(command.retire.iter().copied())
        .collect();
    item_ids.sort_by_key(|id| id.raw());
    let mut owners = BTreeSet::new();
    for id in &item_ids {
        if let Some(owner) = lock_live_owner(tx, *id)? {
            owners.insert(owner);
        }
    }
    let mut actual = domain::affected_characters(command);
    actual.append(&mut owners);
    if actual != locked {
        return Err(PersistError::conflict(
            db_path(),
            "command revisions do not match the characters that own or receive the change",
        ));
    }
    // Retire and park before inserts. A reward can reuse a slot that this
    // command frees, and two live items can exchange slots, while uniqueness
    // still rejects two final occupants.
    for id in &command.retire {
        retire_item(tx, *id)?;
    }
    for item in &command.moves {
        if matches!(item.to, LiveDestination::Character { .. }) {
            move_item(tx, item.item_instance_id, LiveDestination::Ground, rules)?;
        }
    }
    let minted = place_items(tx, command)?;
    for item in &command.moves {
        move_item(tx, item.item_instance_id, item.to, rules)?;
    }
    for write in &command.narrative {
        apply_narrative(tx, write)?;
    }
    for grant in &command.learned {
        let id = id_bytes(grant.character_id.raw());
        let ability = content_i32(grant.ability_content_id)?;
        tx.execute(
            "INSERT INTO character_learned_abilities (character_id, ability_content_id)
             VALUES ($1, $2) ON CONFLICT DO NOTHING",
            &[&id.as_slice() as &(dyn ToSql + Sync), &ability],
        )
        .map_err(map_sql)?;
    }
    let mut revisions = Vec::new();
    for (id, expected) in &expected_ids {
        let next = expected.checked_add(1).ok_or_else(|| {
            PersistError::integrity(db_path(), "persistence_revision cannot advance")
        })?;
        let next_i = revision_i64(next)?;
        let expected_i = revision_i64(*expected)?;
        let raw = id_bytes(id.raw());
        let updated = tx
            .execute(
                "UPDATE characters SET persistence_revision = $2
                 WHERE character_id = $1 AND persistence_revision = $3",
                &[&raw.as_slice() as &(dyn ToSql + Sync), &next_i, &expected_i],
            )
            .map_err(map_sql)?;
        if updated != 1 {
            return Err(PersistError::conflict(
                db_path(),
                "character revision changed during the commit",
            ));
        }
        revisions.push((id.raw(), next));
    }
    let result = DurableCommandResult {
        revisions: revisions
            .iter()
            .map(|(raw, revision)| (CharacterId::from_raw(*raw), *revision))
            .collect(),
        minted_item_ids: minted.clone(),
    };
    let result_json = serde_json::to_string(&StoredResult::from_public(&result))
        .map_err(|err| PersistError::storage(format!("command result encoding failed: {err}")))?;
    tx.execute(
        "INSERT INTO durable_commands (command_key, request_json, result_json) VALUES ($1, $2, $3)",
        &[&command.key, &request, &result_json],
    )
    .map_err(map_sql)?;
    Ok(result)
}

fn locked_command(
    tx: &mut postgres::Transaction<'_>,
    key: &str,
    request: &str,
) -> Result<Option<DurableCommandResult>, PersistError> {
    let row = tx
        .query_opt(
            "SELECT request_json, result_json FROM durable_commands WHERE command_key = $1 FOR UPDATE",
            &[&key],
        )
        .map_err(map_sql)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let stored_request: String = row.get(0);
    let stored_result: String = row.get(1);
    if stored_request != request {
        return Err(PersistError::integrity(
            db_path(),
            "durable command key was reused for a different command",
        ));
    }
    let parsed: StoredResult = serde_json::from_str(&stored_result).map_err(|err| {
        PersistError::integrity(db_path(), format!("stored command result: {err}"))
    })?;
    Ok(Some(parsed.into_public()))
}

fn lock_revision(tx: &mut postgres::Transaction<'_>, id: CharacterId) -> Result<u64, PersistError> {
    let raw = id_bytes(id.raw());
    let row = tx
        .query_opt(
            "SELECT persistence_revision FROM characters WHERE character_id = $1 FOR UPDATE",
            &[&raw.as_slice()],
        )
        .map_err(map_sql)?;
    let Some(row) = row else {
        return Err(PersistError::conflict(
            db_path(),
            format!("character {} is not durable", id.raw()),
        ));
    };
    let revision: i64 = row.get(0);
    u64::try_from(revision)
        .map_err(|_| PersistError::integrity(db_path(), "stored revision is negative"))
}

fn lock_live_owner(
    tx: &mut postgres::Transaction<'_>,
    id: ItemInstanceId,
) -> Result<Option<CharacterId>, PersistError> {
    let raw = id_bytes(id.raw());
    let row = tx
        .query_opt(
            "SELECT state, owner_character_id FROM item_instances WHERE item_instance_id = $1 FOR UPDATE",
            &[&raw.as_slice()],
        )
        .map_err(map_sql)?;
    let Some(row) = row else {
        return Err(PersistError::conflict(
            db_path(),
            format!("item {} does not exist", id.raw()),
        ));
    };
    let state: String = row.get(0);
    if state != "live" {
        return Err(PersistError::conflict(
            db_path(),
            format!("item {} is retired and cannot be reused", id.raw()),
        ));
    }
    let owner: Option<Vec<u8>> = row.get(1);
    match owner {
        Some(bytes) => Ok(Some(CharacterId::from_raw(id_from_bytes(&bytes)?))),
        None => Ok(None),
    }
}

fn place_items(
    tx: &mut postgres::Transaction<'_>,
    command: &DurableCommand,
) -> Result<Vec<ItemInstanceId>, PersistError> {
    if command.place_new.is_empty() {
        return Ok(Vec::new());
    }
    let row = tx
        .query_one(
            "SELECT value FROM durable_meta WHERE key = 'next_item_instance_id'",
            &[],
        )
        .map_err(map_sql)?;
    let text: String = row.get(0);
    let mut next = text
        .parse::<u64>()
        .map_err(|_| PersistError::integrity(db_path(), "next_item_instance_id is corrupt"))?;
    let mut minted = Vec::new();
    for place in &command.place_new {
        if next == 0 {
            return Err(PersistError::ItemIdsExhausted);
        }
        let id = ItemInstanceId::from_raw(next);
        next = next.checked_add(1).ok_or(PersistError::ItemIdsExhausted)?;
        let raw = id_bytes(id.raw());
        let owner = id_bytes(place.owner.raw());
        let definition = content_i32(place.definition_content_id)?;
        let quantity = i32::try_from(place.quantity).map_err(|_| {
            PersistError::content(db_path(), "item quantity exceeds signed 32-bit storage")
        })?;
        let location = location_sql(place.location);
        tx.execute(
            "INSERT INTO item_instances (
                item_instance_id, definition_content_id, quantity, state,
                owner_character_id, location_kind, inventory_slot, equipment_slot
            ) VALUES ($1, $2, $3, 'live', $4, $5, $6, $7)",
            &[
                &raw.as_slice() as &(dyn ToSql + Sync),
                &definition,
                &quantity,
                &owner.as_slice(),
                &location.kind,
                &location.inventory,
                &location.equipment,
            ],
        )
        .map_err(map_sql)?;
        minted.push(id);
    }
    let advanced = next.to_string();
    tx.execute(
        "UPDATE durable_meta SET value = $1 WHERE key = 'next_item_instance_id'",
        &[&advanced],
    )
    .map_err(map_sql)?;
    Ok(minted)
}

struct LocationSql {
    kind: &'static str,
    inventory: Option<i16>,
    equipment: Option<String>,
}

fn location_sql(location: CharacterItemLocation) -> LocationSql {
    match location {
        CharacterItemLocation::Inventory { slot } => LocationSql {
            kind: "inventory",
            inventory: Some(slot as i16),
            equipment: None,
        },
        CharacterItemLocation::Equipped { slot } => LocationSql {
            kind: "equipped",
            inventory: None,
            equipment: Some(slot.as_str().to_string()),
        },
    }
}

fn move_item(
    tx: &mut postgres::Transaction<'_>,
    id: ItemInstanceId,
    to: LiveDestination,
    rules: &DurableContentRules,
) -> Result<(), PersistError> {
    if let LiveDestination::Character { location, .. } = to {
        let (definition, quantity) = committed_item_content(tx, id)?;
        domain::validate_item_content(definition, quantity, location, rules)?;
    }
    let raw = id_bytes(id.raw());
    let (owner, kind, inventory, equipment): (Option<Vec<u8>>, &str, Option<i16>, Option<String>) =
        match to {
            LiveDestination::Ground => (None, "ground", None, None),
            LiveDestination::Character {
                character_id,
                location,
            } => {
                let sql = location_sql(location);
                (
                    Some(id_bytes(character_id.raw()).to_vec()),
                    sql.kind,
                    sql.inventory,
                    sql.equipment,
                )
            }
        };
    let owner_param: Option<&[u8]> = owner.as_deref();
    let updated = tx
        .execute(
            "UPDATE item_instances
             SET owner_character_id = $2, location_kind = $3, inventory_slot = $4, equipment_slot = $5
             WHERE item_instance_id = $1 AND state = 'live'",
            &[
                &raw.as_slice() as &(dyn ToSql + Sync),
                &owner_param,
                &kind,
                &inventory,
                &equipment,
            ],
        )
        .map_err(map_sql)?;
    if updated != 1 {
        return Err(PersistError::conflict(
            db_path(),
            format!("item {} could not move", id.raw()),
        ));
    }
    Ok(())
}

fn committed_item_content(
    tx: &mut postgres::Transaction<'_>,
    id: ItemInstanceId,
) -> Result<(ContentId, u32), PersistError> {
    let raw = id_bytes(id.raw());
    let row = tx
        .query_opt(
            "SELECT definition_content_id, quantity FROM item_instances WHERE item_instance_id = $1",
            &[&raw.as_slice()],
        )
        .map_err(map_sql)?;
    let Some(row) = row else {
        return Err(PersistError::conflict(
            db_path(),
            format!("item {} does not exist", id.raw()),
        ));
    };
    let definition: i32 = row.get(0);
    let quantity: i32 = row.get(1);
    let definition = u32::try_from(definition)
        .map_err(|_| PersistError::integrity(db_path(), "stored item definition is negative"))?;
    let quantity = u32::try_from(quantity)
        .map_err(|_| PersistError::integrity(db_path(), "stored item quantity is negative"))?;
    Ok((ContentId::from_raw(definition), quantity))
}

fn retire_item(tx: &mut postgres::Transaction<'_>, id: ItemInstanceId) -> Result<(), PersistError> {
    let raw = id_bytes(id.raw());
    let updated = tx
        .execute(
            "UPDATE item_instances
             SET state = 'retired', owner_character_id = NULL, location_kind = NULL,
                 inventory_slot = NULL, equipment_slot = NULL
             WHERE item_instance_id = $1 AND state = 'live'",
            &[&raw.as_slice()],
        )
        .map_err(map_sql)?;
    if updated != 1 {
        return Err(PersistError::conflict(
            db_path(),
            format!("item {} could not be retired", id.raw()),
        ));
    }
    Ok(())
}

fn apply_narrative(
    tx: &mut postgres::Transaction<'_>,
    write: &NarrativeWrite,
) -> Result<(), PersistError> {
    match write {
        NarrativeWrite::SetFact {
            character_id,
            fact_key,
            value,
        } => {
            let id = id_bytes(character_id.raw());
            tx.execute(
                "INSERT INTO character_facts (character_id, fact_key, value) VALUES ($1, $2, $3)
                 ON CONFLICT (character_id, fact_key) DO UPDATE SET value = EXCLUDED.value",
                &[
                    &id.as_slice() as &(dyn ToSql + Sync),
                    &fact_key.as_str(),
                    value,
                ],
            )
            .map_err(map_sql)?;
        }
        NarrativeWrite::ClearFact {
            character_id,
            fact_key,
        } => {
            let id = id_bytes(character_id.raw());
            tx.execute(
                "DELETE FROM character_facts WHERE character_id = $1 AND fact_key = $2",
                &[&id.as_slice() as &(dyn ToSql + Sync), &fact_key.as_str()],
            )
            .map_err(map_sql)?;
        }
        NarrativeWrite::MarkNpcMet {
            character_id,
            npc_authored,
        } => {
            let id = id_bytes(character_id.raw());
            tx.execute(
                "INSERT INTO character_npcs_met (character_id, npc_authored) VALUES ($1, $2)
                 ON CONFLICT DO NOTHING",
                &[
                    &id.as_slice() as &(dyn ToSql + Sync),
                    &npc_authored.as_str(),
                ],
            )
            .map_err(map_sql)?;
        }
        NarrativeWrite::MarkDialogueHeard {
            character_id,
            npc_content_id,
            beat_id,
        } => {
            let id = id_bytes(character_id.raw());
            let npc = content_i32(*npc_content_id)?;
            tx.execute(
                "INSERT INTO character_dialogue_heard (character_id, npc_content_id, beat_id)
                 VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
                &[
                    &id.as_slice() as &(dyn ToSql + Sync),
                    &npc,
                    &beat_id.as_str(),
                ],
            )
            .map_err(map_sql)?;
        }
    }
    Ok(())
}

fn command_key_race(err: &PersistError) -> bool {
    matches!(err, PersistError::Conflict { reason, .. } if reason.contains("durable_commands_pkey"))
}

fn commit_reply_lost(err: &PersistError) -> bool {
    matches!(err, PersistError::Storage { reason } if reason.starts_with("commit outcome unknown"))
}

fn ambiguous_commit(err: PersistError) -> PersistError {
    match err {
        PersistError::Storage { reason } => {
            PersistError::storage(format!("commit outcome unknown: {reason}"))
        }
        other => other,
    }
}

fn stored_result_if_same(
    client: &mut Client,
    key: &str,
    request: &str,
) -> Result<Option<DurableCommandResult>, PersistError> {
    let row = client
        .query_opt(
            "SELECT request_json, result_json FROM durable_commands WHERE command_key = $1",
            &[&key],
        )
        .map_err(map_sql)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let stored_request: String = row.get(0);
    if stored_request != request {
        return Err(PersistError::integrity(
            db_path(),
            "durable command key was reused for a different command",
        ));
    }
    let stored_result: String = row.get(1);
    let parsed: StoredResult = serde_json::from_str(&stored_result).map_err(|err| {
        PersistError::integrity(db_path(), format!("stored command result: {err}"))
    })?;
    Ok(Some(parsed.into_public()))
}

fn recover_hidden_commit(
    client: &mut Client,
    key: &str,
    request: &str,
) -> Result<DurableCommandResult, PersistError> {
    match stored_result_if_same(client, key, request)? {
        Some(result) => Ok(result),
        None => Err(PersistError::storage(
            "commit outcome unknown: the commit reply was lost and the command key is not stored",
        )),
    }
}

fn save_restore(
    client: &mut Client,
    snapshot: PersistentCharacterSnapshot,
) -> Result<(), PersistError> {
    let mut tx = client.transaction().map_err(map_sql)?;
    let result = (|| {
        let raw = id_bytes(snapshot.character_id.raw());
        let row = tx
            .query_opt(
                "SELECT persistence_revision, restore_map_authored, restore_point_id,
                        restore_checkpoint_id, instance_exit_reason
                 FROM characters WHERE character_id = $1 FOR UPDATE",
                &[&raw.as_slice()],
            )
            .map_err(map_sql)?;
        let Some(row) = row else {
            return Err(PersistError::corrupt(
                db_path(),
                format!(
                    "cannot save unknown character {}",
                    snapshot.character_id.raw()
                ),
            ));
        };
        let current: i64 = row.get(0);
        let current = u64::try_from(current)
            .map_err(|_| PersistError::integrity(db_path(), "stored revision is negative"))?;
        let stored_map: String = row.get(1);
        let stored_point: String = row.get(2);
        let stored_checkpoint: Option<String> = row.get(3);
        let stored_exit: Option<String> = row.get(4);
        let same_restore = stored_map == snapshot.restore.map_authored
            && stored_point == snapshot.restore.point_id
            && stored_checkpoint.as_deref() == snapshot.restore.checkpoint_id.as_deref()
            && stored_exit.as_deref()
                == snapshot
                    .instance_exit
                    .as_ref()
                    .and_then(|exit| exit.reason.as_deref());
        if snapshot.persistence_revision < current
            || (snapshot.persistence_revision == current && same_restore)
        {
            return Ok(());
        }
        if snapshot.persistence_revision > current {
            let revision = revision_i64(snapshot.persistence_revision)?;
            write_restore(&mut tx, &raw, Some(revision), &snapshot)?;
        } else {
            write_restore(&mut tx, &raw, None, &snapshot)?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => tx.commit().map_err(map_sql),
        Err(err) => {
            let _ = tx.rollback();
            Err(err)
        }
    }
}

fn write_restore(
    tx: &mut postgres::Transaction<'_>,
    raw: &[u8; 8],
    revision: Option<i64>,
    snapshot: &PersistentCharacterSnapshot,
) -> Result<(), PersistError> {
    let checkpoint = snapshot.restore.checkpoint_id.as_deref();
    let exit_reason = snapshot
        .instance_exit
        .as_ref()
        .and_then(|exit| exit.reason.as_deref());
    let updated = if let Some(revision) = revision {
        tx.execute(
            "UPDATE characters
             SET persistence_revision = $2, restore_map_authored = $3, restore_point_id = $4,
                 restore_checkpoint_id = $5, instance_exit_reason = $6
             WHERE character_id = $1",
            &[
                &raw.as_slice() as &(dyn ToSql + Sync),
                &revision,
                &snapshot.restore.map_authored.as_str(),
                &snapshot.restore.point_id.as_str(),
                &checkpoint,
                &exit_reason,
            ],
        )
        .map_err(map_sql)?
    } else {
        tx.execute(
            "UPDATE characters
             SET restore_map_authored = $2, restore_point_id = $3,
                 restore_checkpoint_id = $4, instance_exit_reason = $5
             WHERE character_id = $1",
            &[
                &raw.as_slice() as &(dyn ToSql + Sync),
                &snapshot.restore.map_authored.as_str(),
                &snapshot.restore.point_id.as_str(),
                &checkpoint,
                &exit_reason,
            ],
        )
        .map_err(map_sql)?
    };
    if updated != 1 {
        return Err(PersistError::conflict(
            db_path(),
            "character restore changed during the save",
        ));
    }
    Ok(())
}

fn create_character(
    client: &mut Client,
    login: &DevLogin,
    name: &str,
) -> Result<CharacterRosterEntry, PersistError> {
    let display_name = purgatory_common::CharacterName::parse(name).map_err(|err| {
        PersistError::CreateRejected(crate::error::CreateCharacterRejection::InvalidName(err))
    })?;
    let mut tx = client.transaction().map_err(map_sql)?;
    let result = (|| {
        tx.execute(
            "SELECT value FROM durable_meta WHERE key = 'next_character_id' FOR UPDATE",
            &[],
        )
        .map_err(map_sql)?;
        let count: i64 = tx
            .query_one(
                "SELECT count(*)::bigint FROM characters WHERE owner_login = $1",
                &[&login.as_str()],
            )
            .map_err(map_sql)?
            .get(0);
        if count >= i64::try_from(identity::MAX_ROSTER_SIZE).unwrap_or(i64::MAX) {
            return Err(PersistError::CreateRejected(
                crate::error::CreateCharacterRejection::RosterFull,
            ));
        }
        let name_key = display_name.uniqueness_key();
        let taken: i64 = tx
            .query_one(
                "SELECT count(*)::bigint FROM characters WHERE name_key = $1",
                &[&name_key.as_str()],
            )
            .map_err(map_sql)?
            .get(0);
        if taken > 0 {
            return Err(PersistError::CreateRejected(
                crate::error::CreateCharacterRejection::NameTaken,
            ));
        }
        let next_text: String = tx
            .query_one(
                "SELECT value FROM durable_meta WHERE key = 'next_character_id'",
                &[],
            )
            .map_err(map_sql)?
            .get(0);
        let raw = next_text
            .parse::<u64>()
            .map_err(|_| PersistError::integrity(db_path(), "next_character_id is corrupt"))?;
        if raw == 0 {
            return Err(PersistError::corrupt(db_path(), "next_character_id 0"));
        }
        if raw == u64::MAX {
            let id = id_bytes(raw);
            let exists: i64 = tx
                .query_one(
                    "SELECT count(*)::bigint FROM characters WHERE character_id = $1",
                    &[&id.as_slice()],
                )
                .map_err(map_sql)?
                .get(0);
            if exists > 0 {
                return Err(PersistError::CharacterIdsExhausted);
            }
        }
        let entry = CharacterRosterEntry {
            character_id: CharacterId::from_raw(raw),
            display_name,
        };
        let character = PersistentCharacter::new_default(entry.character_id);
        tx.execute(
            "INSERT INTO dev_users (login) VALUES ($1) ON CONFLICT DO NOTHING",
            &[&login.as_str()],
        )
        .map_err(map_sql)?;
        insert_character_row(
            &mut tx,
            login.as_str(),
            i32::try_from(count).unwrap_or(0),
            &character,
            &entry,
        )?;
        let advanced = raw.saturating_add(1).to_string();
        tx.execute(
            "UPDATE durable_meta SET value = $1 WHERE key = 'next_character_id'",
            &[&advanced],
        )
        .map_err(map_sql)?;
        Ok(entry)
    })();
    match result {
        Ok(entry) => {
            tx.commit().map_err(map_sql)?;
            Ok(entry)
        }
        Err(err) => {
            let _ = tx.rollback();
            Err(err)
        }
    }
}

fn lookup(client: &mut Client, login: &DevLogin) -> Result<Option<CharacterId>, PersistError> {
    let row = client
        .query_opt(
            "SELECT character_id FROM characters WHERE owner_login = $1 ORDER BY roster_position LIMIT 1",
            &[&login.as_str()],
        )
        .map_err(map_sql)?;
    match row {
        Some(row) => {
            let bytes: Vec<u8> = row.get(0);
            Ok(Some(CharacterId::from_raw(id_from_bytes(&bytes)?)))
        }
        None => Ok(None),
    }
}

fn roster(
    client: &mut Client,
    login: &DevLogin,
) -> Result<Vec<CharacterRosterEntry>, PersistError> {
    let rows = client
        .query(
            "SELECT character_id, display_name FROM characters
             WHERE owner_login = $1 ORDER BY roster_position",
            &[&login.as_str()],
        )
        .map_err(map_sql)?;
    let mut roster = Vec::new();
    for row in rows {
        let bytes: Vec<u8> = row.get(0);
        let name: String = row.get(1);
        let display_name = purgatory_common::CharacterName::parse(&name)
            .map_err(|_| PersistError::integrity(db_path(), "stored character name is invalid"))?;
        roster.push(CharacterRosterEntry {
            character_id: CharacterId::from_raw(id_from_bytes(&bytes)?),
            display_name,
        });
    }
    Ok(roster)
}

fn owns(client: &mut Client, login: &DevLogin, id: CharacterId) -> Result<bool, PersistError> {
    let raw = id_bytes(id.raw());
    let count: i64 = client
        .query_one(
            "SELECT count(*)::bigint FROM characters WHERE owner_login = $1 AND character_id = $2",
            &[&login.as_str() as &(dyn ToSql + Sync), &raw.as_slice()],
        )
        .map_err(map_sql)?
        .get(0);
    Ok(count == 1)
}

fn name_keys(client: &mut Client) -> Result<BTreeSet<String>, PersistError> {
    let rows = client
        .query("SELECT name_key FROM characters", &[])
        .map_err(map_sql)?;
    let mut keys = BTreeSet::new();
    for row in rows {
        keys.insert(row.get(0));
    }
    Ok(keys)
}

fn load_character(
    client: &mut Client,
    id: CharacterId,
) -> Result<PersistentCharacter, PersistError> {
    let raw = id_bytes(id.raw());
    let row = client
        .query_opt(
            "SELECT persistence_revision, restore_map_authored, restore_point_id,
                    restore_checkpoint_id, instance_exit_reason
             FROM characters WHERE character_id = $1",
            &[&raw.as_slice()],
        )
        .map_err(map_sql)?;
    let Some(row) = row else {
        return Err(PersistError::corrupt(
            db_path(),
            format!("character {} is missing", id.raw()),
        ));
    };
    let revision: i64 = row.get(0);
    let map_authored: String = row.get(1);
    let point_id: String = row.get(2);
    let checkpoint: Option<String> = row.get(3);
    let exit_reason: Option<String> = row.get(4);
    Ok(PersistentCharacter {
        schema_version: PERSISTENCE_SCHEMA_VERSION,
        character_id: id,
        persistence_revision: u64::try_from(revision)
            .map_err(|_| PersistError::integrity(db_path(), "stored revision is negative"))?,
        restore: purgatory_common::RestoreIntent {
            map_authored,
            point_id,
            checkpoint_id: checkpoint,
        },
        instance_exit: exit_reason.map(|reason| purgatory_common::InstanceExitContext {
            reason: Some(reason),
        }),
    })
}

fn load_item(client: &mut Client, id: ItemInstanceId) -> Result<Option<ItemRecord>, PersistError> {
    let raw = id_bytes(id.raw());
    let row = client
        .query_opt(
            "SELECT definition_content_id, quantity, state, owner_character_id, location_kind,
                    inventory_slot, equipment_slot
             FROM item_instances WHERE item_instance_id = $1",
            &[&raw.as_slice()],
        )
        .map_err(map_sql)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let definition: i32 = row.get(0);
    let quantity: i32 = row.get(1);
    let state: String = row.get(2);
    let owner_bytes: Option<Vec<u8>> = row.get(3);
    let kind: Option<String> = row.get(4);
    let slot: Option<i16> = row.get(5);
    let equipment: Option<String> = row.get(6);
    let owner = match state.as_str() {
        "retired" => ItemOwner::Retired,
        "live" => match kind.as_deref() {
            Some("ground") => ItemOwner::Ground,
            Some("inventory") => {
                let bytes = owner_bytes.ok_or_else(|| {
                    PersistError::integrity(db_path(), "inventory item has no owner")
                })?;
                let slot = slot.ok_or_else(|| {
                    PersistError::integrity(db_path(), "inventory item has no slot")
                })?;
                ItemOwner::Character {
                    character_id: CharacterId::from_raw(id_from_bytes(&bytes)?),
                    location: CharacterItemLocation::Inventory {
                        slot: u16::try_from(slot).map_err(|_| {
                            PersistError::integrity(db_path(), "inventory slot is negative")
                        })?,
                    },
                }
            }
            Some("equipped") => {
                let bytes = owner_bytes.ok_or_else(|| {
                    PersistError::integrity(db_path(), "equipped item has no owner")
                })?;
                let name = equipment.ok_or_else(|| {
                    PersistError::integrity(db_path(), "equipped item has no slot")
                })?;
                let slot = DurableEquipmentSlot::parse(&name).ok_or_else(|| {
                    PersistError::integrity(db_path(), "equipped item slot is unknown")
                })?;
                ItemOwner::Character {
                    character_id: CharacterId::from_raw(id_from_bytes(&bytes)?),
                    location: CharacterItemLocation::Equipped { slot },
                }
            }
            _ => {
                return Err(PersistError::integrity(
                    db_path(),
                    "live item location is unknown",
                ));
            }
        },
        _ => return Err(PersistError::integrity(db_path(), "item state is unknown")),
    };
    let definition = u32::try_from(definition)
        .map_err(|_| PersistError::integrity(db_path(), "item content id is negative"))?;
    let quantity = u32::try_from(quantity)
        .map_err(|_| PersistError::integrity(db_path(), "item quantity is negative"))?;
    Ok(Some(ItemRecord {
        item_instance_id: id,
        definition_content_id: ContentId::from_raw(definition),
        quantity,
        owner,
    }))
}

fn load_narrative(
    client: &mut Client,
    id: CharacterId,
) -> Result<CharacterNarrativeState, PersistError> {
    let raw = id_bytes(id.raw());
    let mut state = CharacterNarrativeState::default();
    for row in client
        .query(
            "SELECT fact_key, value FROM character_facts WHERE character_id = $1 ORDER BY fact_key",
            &[&raw.as_slice()],
        )
        .map_err(map_sql)?
    {
        state.facts.insert(row.get(0), row.get(1));
    }
    for row in client
        .query(
            "SELECT npc_authored FROM character_npcs_met WHERE character_id = $1 ORDER BY npc_authored",
            &[&raw.as_slice()],
        )
        .map_err(map_sql)?
    {
        state.npcs_met.insert(row.get(0));
    }
    for row in client
        .query(
            "SELECT npc_content_id, beat_id FROM character_dialogue_heard
             WHERE character_id = $1 ORDER BY npc_content_id, beat_id",
            &[&raw.as_slice()],
        )
        .map_err(map_sql)?
    {
        let npc: i32 = row.get(0);
        let beat: String = row.get(1);
        let npc = u32::try_from(npc)
            .map_err(|_| PersistError::integrity(db_path(), "dialogue npc id is negative"))?;
        state.dialogue_heard.insert((npc, beat));
    }
    for row in client
        .query(
            "SELECT ability_content_id FROM character_learned_abilities
             WHERE character_id = $1 ORDER BY ability_content_id",
            &[&raw.as_slice()],
        )
        .map_err(map_sql)?
    {
        let ability: i32 = row.get(0);
        state.learned_abilities.insert(
            u32::try_from(ability)
                .map_err(|_| PersistError::integrity(db_path(), "ability id is negative"))?,
        );
    }
    Ok(state)
}

fn id_bytes(raw: u64) -> [u8; 8] {
    raw.to_be_bytes()
}

fn id_from_bytes(bytes: &[u8]) -> Result<u64, PersistError> {
    let array: [u8; 8] = bytes
        .try_into()
        .map_err(|_| PersistError::integrity(db_path(), "stored identity width is not 8 bytes"))?;
    Ok(u64::from_be_bytes(array))
}

fn revision_i64(value: u64) -> Result<i64, PersistError> {
    i64::try_from(value).map_err(|_| {
        PersistError::integrity(
            db_path(),
            "persistence_revision exceeds signed 64-bit storage",
        )
    })
}

fn content_i32(id: ContentId) -> Result<i32, PersistError> {
    let raw = id.raw().ok_or_else(|| {
        PersistError::content(db_path(), "content id is outside the numeric catalog")
    })?;
    i32::try_from(raw).map_err(|_| PersistError::content(db_path(), "content id does not fit"))
}

fn map_sql(err: postgres::Error) -> PersistError {
    if let Some(db) = err.as_db_error() {
        let constraint = db.constraint().unwrap_or("constraint");
        if db.code().code() == "23505" {
            return PersistError::conflict(db_path(), format!("unique constraint {constraint}"));
        }
        if db.code().code() == "23514" {
            return PersistError::conflict(db_path(), format!("check constraint {constraint}"));
        }
        return PersistError::storage(sanitize(db.message()));
    }
    PersistError::storage(sanitize(&err.to_string()))
}

fn sanitize(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    if text.contains("://") || text.contains('@') || lower.contains("password") {
        return "database operation failed".into();
    }
    let mut out = text.replace(['\n', '\r'], " ");
    if out.len() > 400 {
        out.truncate(400);
    }
    out
}

#[derive(Serialize, Deserialize)]
struct StoredResult {
    revisions: Vec<(u64, u64)>,
    minted_item_ids: Vec<u64>,
}

impl StoredResult {
    fn from_public(result: &DurableCommandResult) -> Self {
        Self {
            revisions: result
                .revisions
                .iter()
                .map(|(id, revision)| (id.raw(), *revision))
                .collect(),
            minted_item_ids: result.minted_item_ids.iter().map(|id| id.raw()).collect(),
        }
    }

    fn into_public(self) -> DurableCommandResult {
        DurableCommandResult {
            revisions: self
                .revisions
                .into_iter()
                .map(|(raw, revision)| (CharacterId::from_raw(raw), revision))
                .collect(),
            minted_item_ids: self
                .minted_item_ids
                .into_iter()
                .map(ItemInstanceId::from_raw)
                .collect(),
        }
    }
}

fn canonical_request(command: &DurableCommand) -> Result<String, PersistError> {
    let canon = CanonCommand {
        key: command.key.clone(),
        expected_revisions: command
            .expected_revisions
            .iter()
            .map(|(id, revision)| (id.raw(), *revision))
            .collect(),
        place_new: command
            .place_new
            .iter()
            .map(|place| CanonPlace {
                owner: place.owner.raw(),
                definition: place.definition_content_id.token(),
                quantity: place.quantity,
                location: canon_location(place.location),
            })
            .collect(),
        moves: command
            .moves
            .iter()
            .map(|item| CanonMove {
                item_instance_id: item.item_instance_id.raw(),
                to: match item.to {
                    LiveDestination::Ground => CanonDestination::Ground,
                    LiveDestination::Character {
                        character_id,
                        location,
                    } => CanonDestination::Character {
                        character_id: character_id.raw(),
                        location: canon_location(location),
                    },
                },
            })
            .collect(),
        retire: command.retire.iter().map(|id| id.raw()).collect(),
        narrative: command.narrative.iter().map(canon_narrative).collect(),
        learned: command
            .learned
            .iter()
            .map(|grant| (grant.character_id.raw(), grant.ability_content_id.token()))
            .collect(),
    };
    serde_json::to_string(&canon)
        .map_err(|err| PersistError::storage(format!("command encoding failed: {err}")))
}

fn canon_location(location: CharacterItemLocation) -> CanonLocation {
    match location {
        CharacterItemLocation::Inventory { slot } => CanonLocation::Inventory { slot },
        CharacterItemLocation::Equipped { slot } => CanonLocation::Equipped {
            slot: slot.as_str(),
        },
    }
}

fn canon_narrative(write: &NarrativeWrite) -> CanonNarrative {
    match write {
        NarrativeWrite::SetFact {
            character_id,
            fact_key,
            value,
        } => CanonNarrative::SetFact {
            character_id: character_id.raw(),
            fact_key: fact_key.clone(),
            value: *value,
        },
        NarrativeWrite::ClearFact {
            character_id,
            fact_key,
        } => CanonNarrative::ClearFact {
            character_id: character_id.raw(),
            fact_key: fact_key.clone(),
        },
        NarrativeWrite::MarkNpcMet {
            character_id,
            npc_authored,
        } => CanonNarrative::MarkNpcMet {
            character_id: character_id.raw(),
            npc_authored: npc_authored.clone(),
        },
        NarrativeWrite::MarkDialogueHeard {
            character_id,
            npc_content_id,
            beat_id,
        } => CanonNarrative::MarkDialogueHeard {
            character_id: character_id.raw(),
            npc_content_id: npc_content_id.token(),
            beat_id: beat_id.clone(),
        },
    }
}

#[derive(Serialize)]
struct CanonCommand {
    key: String,
    expected_revisions: Vec<(u64, u64)>,
    place_new: Vec<CanonPlace>,
    moves: Vec<CanonMove>,
    retire: Vec<u64>,
    narrative: Vec<CanonNarrative>,
    learned: Vec<(u64, u64)>,
}

#[derive(Serialize)]
struct CanonPlace {
    owner: u64,
    definition: u64,
    quantity: u32,
    location: CanonLocation,
}

#[derive(Serialize)]
struct CanonMove {
    item_instance_id: u64,
    to: CanonDestination,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CanonDestination {
    Ground,
    Character {
        character_id: u64,
        location: CanonLocation,
    },
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CanonLocation {
    Inventory { slot: u16 },
    Equipped { slot: &'static str },
}

#[derive(Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum CanonNarrative {
    SetFact {
        character_id: u64,
        fact_key: String,
        value: bool,
    },
    ClearFact {
        character_id: u64,
        fact_key: String,
    },
    MarkNpcMet {
        character_id: u64,
        npc_authored: String,
    },
    MarkDialogueHeard {
        character_id: u64,
        npc_content_id: u64,
        beat_id: String,
    },
}

#[cfg(test)]
pub(crate) struct EphemeralRoles {
    pub runtime: PostgresSettings,
    admin_url: String,
    migration_role: String,
    runtime_role: String,
}

#[cfg(test)]
impl Drop for EphemeralRoles {
    fn drop(&mut self) {
        if let Err(err) =
            release_ephemeral_roles(&self.admin_url, &self.migration_role, &self.runtime_role)
        {
            eprintln!("PURGATORY ephemeral role cleanup failed: {err}");
        }
    }
}

#[cfg(test)]
pub(crate) fn provision_ephemeral_roles(
    admin: &PostgresSettings,
) -> Result<EphemeralRoles, PersistError> {
    let mut client = connect_url(&admin.url)?;
    let seq = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let migration_role = format!("p12a_m_{}_{seq}", std::process::id());
    let runtime_role = format!("p12a_r_{}_{seq}", std::process::id());
    let password = format!("p12a{seq}");
    for role in [&migration_role, &runtime_role] {
        let sql: String = client
            .query_one(
                "SELECT format('CREATE ROLE %I LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT PASSWORD %L', $1::text, $2::text)",
                &[role, &password],
            )
            .map_err(map_sql)?
            .get(0);
        client.batch_execute(&sql).map_err(map_sql)?;
    }
    let database = database_name(&admin.url)?;
    let grant_create: String = client
        .query_one(
            "SELECT format('GRANT CREATE ON DATABASE %I TO %I', $1::text, $2::text)",
            &[&database, &migration_role],
        )
        .map_err(map_sql)?
        .get(0);
    client.batch_execute(&grant_create).map_err(map_sql)?;
    let runtime = PostgresSettings::for_tests(
        replace_userinfo(&admin.url, &runtime_role, &password)?,
        admin.schema.clone(),
    )?
    .with_migration_url(replace_userinfo(&admin.url, &migration_role, &password)?)?;
    Ok(EphemeralRoles {
        runtime,
        admin_url: admin.url.clone(),
        migration_role,
        runtime_role,
    })
}

#[cfg(test)]
fn release_ephemeral_roles(
    admin_url: &str,
    migration_role: &str,
    runtime_role: &str,
) -> Result<(), PersistError> {
    let mut client = connect_url(admin_url)?;
    for role in [migration_role, runtime_role] {
        let exists: bool = client
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM pg_roles WHERE rolname = $1)",
                &[&role],
            )
            .map_err(map_sql)?
            .get(0);
        if !exists {
            continue;
        }
        let reassign: String = client
            .query_one(
                "SELECT format('REASSIGN OWNED BY %I TO CURRENT_USER', $1::text)",
                &[&role],
            )
            .map_err(map_sql)?
            .get(0);
        client.batch_execute(&reassign).map_err(map_sql)?;
        let drop_owned: String = client
            .query_one("SELECT format('DROP OWNED BY %I', $1::text)", &[&role])
            .map_err(map_sql)?
            .get(0);
        client.batch_execute(&drop_owned).map_err(map_sql)?;
        let drop_role: String = client
            .query_one("SELECT format('DROP ROLE %I', $1::text)", &[&role])
            .map_err(map_sql)?
            .get(0);
        client.batch_execute(&drop_role).map_err(map_sql)?;
    }
    Ok(())
}

#[cfg(test)]
fn replace_userinfo(url: &str, user: &str, password: &str) -> Result<String, PersistError> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| PersistError::storage("invalid database url"))?;
    let after_user = rest.split_once('@').map(|(_, after)| after).unwrap_or(rest);
    Ok(format!("{scheme}://{user}:{password}@{after_user}"))
}

#[cfg(test)]
pub(crate) fn runtime_cannot_create_table(
    settings: &PostgresSettings,
) -> Result<bool, PersistError> {
    let mut client = connect_url(&settings.url)?;
    prepare_schema(&mut client, &settings.schema, false)?;
    match client.batch_execute("CREATE TABLE runtime_must_not_create (id integer)") {
        Ok(()) => Ok(false),
        Err(err) => {
            let mapped = map_sql(err);
            let text = mapped.to_string().to_ascii_lowercase();
            if text.contains("permission denied") || text.contains("must be owner") {
                Ok(true)
            } else {
                Err(mapped)
            }
        }
    }
}
