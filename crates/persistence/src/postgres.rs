//! Synchronous PostgreSQL writer used by the persistence worker.
//!
//! The client is blocking. Callers are the persistence worker thread, never the
//! 30 Hz simulation tick. One connection serves one service. Channels are not
//! databases and not schemas.

use std::collections::BTreeSet;
use std::str::FromStr;
use std::time::Duration;

use postgres::types::ToSql;
use postgres::{Client, Config, NoTls};
use purgatory_common::{CharacterId, ContentId, DevLogin, ItemInstanceId};
use serde::Deserialize;
use serde::Serialize;

use crate::character::{
    PERSISTENCE_SCHEMA_VERSION, PersistentCharacter, PersistentCharacterSnapshot,
};
use crate::domain::{
    self, CharacterItemLocation, CharacterNarrativeState, DurableCommand, DurableCommandResult,
    DurableContentRules, DurableEquipmentSlot, ItemOwner, ItemRecord, LiveDestination,
    NarrativeWrite, ReservedItemOutcome, db_path,
};
use crate::error::PersistError;
use crate::identity::{self, CharacterRosterEntry};
use crate::lifecycle::{self, Admission, ChannelClaim, LeaseAuthority, LeaseBarrier, OwnedRestore};

const MIGRATIONS: &[(i32, &str)] = &[
    (1, include_str!("../migrations/0001_foundation.sql")),
    (2, include_str!("../migrations/0002_lifecycle.sql")),
    (
        3,
        include_str!("../migrations/0003_item_id_reservations.sql"),
    ),
    (4, include_str!("../migrations/0004_current_health.sql")),
];
const IMPORT_LOCK_KEY: i64 = 0x120A_0001;
/// Reserved readiness login. It is not inserted into `dev_users` and it is not
/// a player account.
pub(crate) const DEVELOPMENT_PROBE_LOGIN: &str = "dev.probe";
const RESERVED_DATABASES: &[&str] = &["purgatory_dev", "postgres", "template0", "template1"];

#[derive(Clone)]
pub struct PostgresSettings {
    pub url: String,
    pub migration_url: Option<String>,
    pub schema: String,
    /// Stable name of this world deployment. Stored in the database at
    /// bootstrap and checked on every later open.
    pub deployment_id: String,
}

impl PostgresSettings {
    pub fn from_env() -> Result<Option<Self>, PersistError> {
        if let Ok(url) = std::env::var("PURGATORY_DATABASE_URL")
            && !url.trim().is_empty()
        {
            return Self::from_vars(|key| std::env::var(key));
        }
        match crate::local_config::discover_process_file() {
            Ok(Some(config)) => Ok(Some(config.runtime_settings())),
            Ok(None) => Self::from_vars(|key| std::env::var(key)),
            Err(err) => Err(PersistError::storage(err.to_string())),
        }
    }

    /// `start` is a workspace directory that may contain the local file.
    /// An explicit `PURGATORY_DATABASE_URL` from `getenv` wins and the file is ignored.
    #[cfg(test)]
    pub(crate) fn from_env_in(
        start: Option<&std::path::Path>,
        mut getenv: impl FnMut(&str) -> Result<String, std::env::VarError>,
    ) -> Result<Option<Self>, PersistError> {
        if let Ok(url) = getenv("PURGATORY_DATABASE_URL")
            && !url.trim().is_empty()
        {
            return Self::from_vars(getenv);
        }
        if let Some(start) = start {
            match crate::local_config::discover(start) {
                Ok(Some(config)) => return Ok(Some(config.runtime_settings())),
                Ok(None) => {}
                Err(err) => return Err(PersistError::storage(err.to_string())),
            }
        }
        Self::from_vars(getenv)
    }

    pub(crate) fn from_vars(
        mut getenv: impl FnMut(&str) -> Result<String, std::env::VarError>,
    ) -> Result<Option<Self>, PersistError> {
        let url = match getenv("PURGATORY_DATABASE_URL") {
            Ok(url) => url,
            Err(std::env::VarError::NotPresent) => return Ok(None),
            Err(err) => {
                return Err(PersistError::storage(format!(
                    "PURGATORY_DATABASE_URL is not valid unicode: {err}"
                )));
            }
        };
        let url = url.trim();
        if url.is_empty() {
            return Ok(None);
        }
        let deployment_id = match getenv("PURGATORY_DEPLOYMENT_ID") {
            Ok(value) => value.trim().to_string(),
            Err(std::env::VarError::NotPresent) => String::new(),
            Err(err) => {
                return Err(PersistError::storage(format!(
                    "PURGATORY_DEPLOYMENT_ID is not valid unicode: {err}"
                )));
            }
        };
        if deployment_id.is_empty() {
            return Err(PersistError::storage(
                "PURGATORY_DEPLOYMENT_ID is required when PostgreSQL is configured",
            ));
        }
        validate_deployment_id(&deployment_id)?;
        let migration_url = match getenv("PURGATORY_DATABASE_MIGRATION_URL") {
            Ok(value) => {
                let value = value.trim().to_string();
                if value.is_empty() { None } else { Some(value) }
            }
            Err(_) => None,
        };
        let schema = match getenv("PURGATORY_DATABASE_SCHEMA") {
            Ok(value) => {
                let value = value.trim().to_string();
                if value.is_empty() {
                    "public".into()
                } else {
                    value
                }
            }
            Err(_) => "public".into(),
        };
        validated_schema(&schema)?;
        Ok(Some(Self {
            url: url.to_string(),
            migration_url,
            schema,
            deployment_id,
        }))
    }

    /// Test connections must name a dedicated database and a disposable schema.
    /// `purgatory_dev` is refused before any connection is opened.
    pub fn for_tests(url: String, schema: String) -> Result<Self, PersistError> {
        let settings = Self {
            url,
            migration_url: None,
            deployment_id: schema.clone(),
            schema,
        };
        validate_deployment_id(&settings.deployment_id)?;
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
            deployment_id: self.deployment_id.clone(),
        };
        migration.refuse_reserved_database()?;
        self.migration_url = Some(migration.url);
        Ok(self)
    }

    /// Connect with the runtime role and require an initialized deployment.
    /// Does not migrate.
    pub(crate) fn open_check(settings: &Self) -> Result<(), PersistError> {
        let _store = PostgresStore::open(settings)?;
        Ok(())
    }

    pub fn with_deployment_id(mut self, deployment_id: String) -> Result<Self, PersistError> {
        validate_deployment_id(&deployment_id)?;
        self.deployment_id = deployment_id;
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
    /// Runtime URL used to replace a connection that died after COMMIT.
    runtime_url: String,
    schema: String,
    rules: DurableContentRules,
    #[cfg(test)]
    hide_commit_reply: bool,
    #[cfg(test)]
    discard_connection_after_commit: bool,
    /// The current client cannot be used. The next call opens another one.
    connection_closed: bool,
    #[cfg(test)]
    reconnect_failures_remaining: u32,
    /// Channel generations this process claimed. Ground writes stamp one of them.
    channels: std::collections::BTreeMap<i64, u64>,
    #[cfg(test)]
    lease_barrier: Option<LeaseBarrier>,
}

impl PostgresStore {
    /// Reopen a database that was already bootstrapped. Does not create a
    /// schema, import files, or write a local marker.
    pub(crate) fn open(settings: &PostgresSettings) -> Result<Self, PersistError> {
        let mut client = connect_prepared(settings, false)?;
        require_deployment(&mut client, &settings.deployment_id)?;
        warn_durability(&mut client);
        Ok(store_from_client(client, settings))
    }

    /// One-time empty roster inside an existing database. Creates the schema
    /// and migrations, then records the deployment identity. Does not create
    /// the physical database and does not read legacy files.
    pub(crate) fn bootstrap(settings: &PostgresSettings) -> Result<(), PersistError> {
        validate_deployment_id(&settings.deployment_id)?;
        let mut client = connect_prepared(settings, true)?;
        // A distinct migration role already initialized the empty world on its
        // own connection. The runtime role cannot insert development users.
        if settings.migration_url.is_none() {
            bootstrap_empty(&mut client, &settings.deployment_id)?;
        }
        Ok(())
    }

    pub(crate) fn set_rules(&mut self, rules: DurableContentRules) {
        self.rules = rules;
    }

    pub(crate) fn read_item(
        &mut self,
        id: ItemInstanceId,
    ) -> Result<Option<ItemRecord>, PersistError> {
        load_item(&mut self.client, id)
    }

    pub(crate) fn read_owned_restore(
        &mut self,
        id: CharacterId,
    ) -> Result<OwnedRestore, PersistError> {
        self.ensure_connection()?;
        let err = match self.client.transaction() {
            Ok(mut tx) => {
                let restore = crate::lifecycle::load_restore(&mut tx, id)?;
                tx.rollback().map_err(map_sql)?;
                return Ok(restore);
            }
            Err(err) => err,
        };
        self.note_closed_client();
        Err(map_sql(err))
    }

    pub(crate) fn commit(
        &mut self,
        command: &DurableCommand,
        lease: Option<&LeaseAuthority>,
    ) -> Result<DurableCommandResult, PersistError> {
        self.ensure_connection()?;
        let request = canonical_request(command)?;
        // A stored result is returned before content rules are applied again.
        // Retry after a catalog change must still answer the original commit.
        let stored = match stored_result_if_same(&mut self.client, &command.key, &request) {
            Ok(value) => value,
            Err(err) if self.client.is_closed() => {
                self.connection_closed = true;
                return Err(PersistError::storage(format!(
                    "commit outcome unknown: {err}"
                )));
            }
            Err(err) => return Err(err),
        };
        if let Some(result) = stored {
            return Ok(result);
        }
        domain::validate_command(command, &self.rules)?;
        let channels = self.channels.clone();
        let mut barrier = self.take_barrier();
        match commit_once(
            &mut self.client,
            command,
            &request,
            &self.rules,
            lease,
            &channels,
            &mut barrier,
        ) {
            Ok(result) => {
                if self.consume_hidden_reply() {
                    return recover_hidden_commit(&mut self.client, &command.key, &request);
                }
                #[cfg(test)]
                if self.consume_discard_connection() {
                    return recover_on_unusable_connection(
                        &mut self.client,
                        &mut self.connection_closed,
                    );
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
            Err(err) => {
                if self.client.is_closed() {
                    self.connection_closed = true;
                    if self.ensure_connection().is_ok()
                        && let Ok(Some(result)) =
                            stored_result_if_same(&mut self.client, &command.key, &request)
                    {
                        return Ok(result);
                    }
                    return Err(PersistError::storage(format!(
                        "commit outcome unknown: {err}"
                    )));
                }
                Err(err)
            }
        }
    }

    /// Advance `next_item_instance_id` and record that range for the process's
    /// live channel generation. A crash after this commit wastes unused ids.
    /// A crash before it issues none. The same counter feeds `place_new`, so
    /// the ranges stay disjoint. A later generation cannot spend this range.
    pub(crate) fn reserve_item_ids(
        &mut self,
        count: u32,
    ) -> Result<Vec<ItemInstanceId>, PersistError> {
        if count == 0 || count > 256 {
            return Err(PersistError::corrupt(
                db_path(),
                "item id reservation count is invalid",
            ));
        }
        let (channel_id, generation) = reservation_channel(&self.channels)?;
        self.ensure_connection()?;
        let mut tx = self.client.transaction().map_err(map_sql)?;
        tx.execute(
            "SELECT value FROM durable_meta WHERE key = 'next_item_instance_id' FOR UPDATE",
            &[],
        )
        .map_err(map_sql)?;
        lifecycle::lock_live_channel(&mut tx, channel_id, generation)?;
        let start = read_next_item_id(&mut tx)?;
        let end = start
            .checked_add(u64::from(count))
            .ok_or(PersistError::ItemIdsExhausted)?;
        if start == 0 {
            return Err(PersistError::ItemIdsExhausted);
        }
        let start_sql = item_id_i64(start)?;
        let end_sql = item_id_i64(end)?;
        let generation_sql = revision_i64(generation)?;
        tx.execute(
            "INSERT INTO item_id_reservations (range_start, range_end, channel_id, generation)
             VALUES ($1, $2, $3, $4)",
            &[&start_sql, &end_sql, &channel_id, &generation_sql],
        )
        .map_err(map_sql)?;
        tx.execute(
            "UPDATE durable_meta SET value = $1 WHERE key = 'next_item_instance_id'",
            &[&end.to_string()],
        )
        .map_err(map_sql)?;
        tx.commit().map_err(|err| {
            if self.client.is_closed() {
                self.connection_closed = true;
            }
            map_sql(err)
        })?;
        Ok((start..end).map(ItemInstanceId::from_raw).collect())
    }

    /// Raise the item-id counter without recording a reservation. Tests use
    /// this to prove an unissued gap cannot be inserted.
    #[cfg(test)]
    pub(crate) fn leave_unissued_item_gap_for_test(
        &mut self,
        next: u64,
    ) -> Result<(), PersistError> {
        if next <= 1 {
            return Err(PersistError::corrupt(
                db_path(),
                "unissued item gap must end above 1",
            ));
        }
        self.ensure_connection()?;
        let mut tx = self.client.transaction().map_err(map_sql)?;
        tx.execute(
            "SELECT value FROM durable_meta WHERE key = 'next_item_instance_id' FOR UPDATE",
            &[],
        )
        .map_err(map_sql)?;
        let current = read_next_item_id(&mut tx)?;
        if next < current {
            return Err(PersistError::corrupt(
                db_path(),
                "unissued item gap cannot rewind the counter",
            ));
        }
        tx.execute(
            "UPDATE durable_meta SET value = $1 WHERE key = 'next_item_instance_id'",
            &[&next.to_string()],
        )
        .map_err(map_sql)?;
        tx.commit().map_err(map_sql)?;
        Ok(())
    }

    fn note_closed_client(&mut self) {
        if self.client.is_closed() {
            self.connection_closed = true;
        }
    }

    /// Replace a dead client. A failed replacement stays unknown so a committed
    /// command is not reported as a definite rejection.
    fn ensure_connection(&mut self) -> Result<(), PersistError> {
        if !self.connection_closed && !self.client.is_closed() {
            return Ok(());
        }
        self.connection_closed = true;
        #[cfg(test)]
        if self.reconnect_failures_remaining > 0 {
            self.reconnect_failures_remaining -= 1;
            return Err(PersistError::storage(
                "commit outcome unknown: the database connection is closed",
            ));
        }
        let mut client = connect_url(&self.runtime_url)
            .map_err(|err| PersistError::storage(format!("commit outcome unknown: {err}")))?;
        prepare_schema(&mut client, &self.schema, false)
            .map_err(|err| PersistError::storage(format!("commit outcome unknown: {err}")))?;
        self.client = client;
        self.connection_closed = false;
        Ok(())
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
    fn consume_discard_connection(&mut self) -> bool {
        let discard = self.discard_connection_after_commit;
        self.discard_connection_after_commit = false;
        discard
    }

    #[cfg(test)]
    pub(crate) fn hide_next_commit_reply(&mut self) {
        self.hide_commit_reply = true;
    }

    #[cfg(test)]
    pub(crate) fn discard_connection_after_next_commit(&mut self) {
        self.discard_connection_after_commit = true;
    }

    #[cfg(test)]
    pub(crate) fn fail_next_reconnects(&mut self, count: u32) {
        self.reconnect_failures_remaining = count;
    }

    pub(crate) fn save_restore(
        &mut self,
        snapshot: PersistentCharacterSnapshot,
        lease: Option<&LeaseAuthority>,
    ) -> Result<(), PersistError> {
        let mut barrier = self.take_barrier();
        save_restore(&mut self.client, snapshot, lease, &mut barrier)
    }

    fn take_barrier(&mut self) -> Option<LeaseBarrier> {
        #[cfg(test)]
        {
            self.lease_barrier.take()
        }
        #[cfg(not(test))]
        {
            None
        }
    }

    #[cfg(test)]
    pub(crate) fn set_lease_barrier(&mut self, barrier: LeaseBarrier) {
        self.lease_barrier = Some(barrier);
    }

    pub(crate) fn admit(
        &mut self,
        login: &DevLogin,
        character_id: CharacterId,
    ) -> Result<Admission, PersistError> {
        for _ in 0..2 {
            let mut barrier = self.take_barrier();
            let mut tx = self.client.transaction().map_err(map_sql)?;
            match lifecycle::admit(&mut tx, login, character_id, &mut barrier) {
                Ok(admission) => {
                    tx.commit().map_err(|err| ambiguous_commit(map_sql(err)))?;
                    return Ok(admission);
                }
                Err(err) if lease_insert_raced(&err) => {
                    let _ = tx.rollback();
                }
                Err(err) => {
                    let _ = tx.rollback();
                    return Err(err);
                }
            }
        }
        Err(PersistError::conflict(
            db_path(),
            "character lease insert raced",
        ))
    }

    pub(crate) fn supersede(
        &mut self,
        authority: &LeaseAuthority,
    ) -> Result<(LeaseAuthority, OwnedRestore), PersistError> {
        let mut barrier = self.take_barrier();
        let mut tx = self.client.transaction().map_err(map_sql)?;
        match lifecycle::supersede(&mut tx, authority, &mut barrier) {
            Ok(next) => {
                tx.commit().map_err(map_sql)?;
                Ok(next)
            }
            Err(err) => {
                let _ = tx.rollback();
                Err(err)
            }
        }
    }

    pub(crate) fn renew_lease(&mut self, authority: &LeaseAuthority) -> Result<(), PersistError> {
        let mut barrier = self.take_barrier();
        let mut tx = self.client.transaction().map_err(map_sql)?;
        match lifecycle::renew(&mut tx, authority, &mut barrier) {
            Ok(()) => tx.commit().map_err(map_sql),
            Err(err) => {
                let _ = tx.rollback();
                Err(err)
            }
        }
    }

    pub(crate) fn release_lease(&mut self, authority: &LeaseAuthority) -> Result<(), PersistError> {
        let mut barrier = self.take_barrier();
        let mut tx = self.client.transaction().map_err(map_sql)?;
        match lifecycle::release(&mut tx, authority, &mut barrier) {
            Ok(()) => tx.commit().map_err(map_sql),
            Err(err) => {
                let _ = tx.rollback();
                Err(err)
            }
        }
    }

    pub(crate) fn claim_channel(
        &mut self,
        channel_id: i64,
        retire_limit: Option<i64>,
    ) -> Result<ChannelClaim, PersistError> {
        let mut barrier = self.take_barrier();
        let mut tx = self.client.transaction().map_err(map_sql)?;
        match lifecycle::claim_channel(&mut tx, channel_id, retire_limit, &mut barrier) {
            Ok(claim) => {
                tx.commit().map_err(map_sql)?;
                if let ChannelClaim::Claimed { generation, .. } = claim {
                    self.channels.insert(channel_id, generation);
                }
                Ok(claim)
            }
            Err(err) => {
                let _ = tx.rollback();
                Err(err)
            }
        }
    }

    pub(crate) fn sweep_channel(
        &mut self,
        channel_id: i64,
        generation: u64,
        retire_limit: Option<i64>,
    ) -> Result<u64, PersistError> {
        let mut tx = self.client.transaction().map_err(map_sql)?;
        match lifecycle::sweep_held_channel(&mut tx, channel_id, generation, retire_limit) {
            Ok(retired) => {
                tx.commit().map_err(map_sql)?;
                Ok(retired)
            }
            Err(err) => {
                let _ = tx.rollback();
                Err(err)
            }
        }
    }

    pub(crate) fn renew_channel(
        &mut self,
        channel_id: i64,
        generation: u64,
    ) -> Result<(), PersistError> {
        let mut tx = self.client.transaction().map_err(map_sql)?;
        match lifecycle::renew_channel(&mut tx, channel_id, generation) {
            Ok(()) => {
                tx.commit().map_err(map_sql)?;
                self.channels.insert(channel_id, generation);
                Ok(())
            }
            Err(err) => {
                let _ = tx.rollback();
                self.channels.remove(&channel_id);
                Err(err)
            }
        }
    }

    pub(crate) fn release_channel(
        &mut self,
        channel_id: i64,
        generation: u64,
    ) -> Result<(), PersistError> {
        let mut tx = self.client.transaction().map_err(map_sql)?;
        match lifecycle::release_channel(&mut tx, channel_id, generation) {
            Ok(()) => {
                tx.commit().map_err(map_sql)?;
                self.channels.remove(&channel_id);
                Ok(())
            }
            Err(err) => {
                let _ = tx.rollback();
                Err(err)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn clock_moved_inside_one_statement(
        &mut self,
    ) -> Result<(bool, bool), PersistError> {
        let row = self
            .client
            .query_one(
                "SELECT now() = transaction_timestamp(), clock_timestamp() > now()
                 FROM (SELECT pg_sleep(0.2)) AS delay",
                &[],
            )
            .map_err(map_sql)?;
        Ok((row.get(0), row.get(1)))
    }

    #[cfg(test)]
    pub(crate) fn sessions_waiting_on_a_lock(&mut self) -> Result<i64, PersistError> {
        let count: i64 = self
            .client
            .query_one(
                "SELECT count(*)::bigint FROM pg_stat_activity
                 WHERE wait_event_type = 'Lock' AND pid <> pg_backend_pid()",
                &[],
            )
            .map_err(map_sql)?
            .get(0);
        Ok(count)
    }

    #[cfg(test)]
    pub(crate) fn remember_channel_for_test(&mut self, channel_id: i64, generation: u64) {
        self.channels.insert(channel_id, generation);
    }

    #[cfg(test)]
    pub(crate) fn expire_lease_for_test(&mut self, login: &DevLogin) -> Result<(), PersistError> {
        self.client
            .execute(
                "UPDATE character_leases
                 SET expires_at = clock_timestamp() - interval '1 second'
                 WHERE owner_login = $1",
                &[&login.as_str()],
            )
            .map_err(map_sql)?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn expire_channel_for_test(&mut self, channel_id: i64) -> Result<(), PersistError> {
        self.client
            .execute(
                "UPDATE channel_generations
                 SET expires_at = clock_timestamp() - interval '1 second'
                 WHERE channel_id = $1",
                &[&channel_id],
            )
            .map_err(map_sql)?;
        Ok(())
    }

    /// Leave a live ground row with no channel stamp. Production ground writes
    /// stamp the one claimed channel; this only recreates a legacy row.
    #[cfg(test)]
    pub(crate) fn unstamp_ground_for_test(
        &mut self,
        id: ItemInstanceId,
    ) -> Result<(), PersistError> {
        let raw = id_bytes(id.raw());
        let updated = self
            .client
            .execute(
                "UPDATE item_instances
                 SET ground_channel_id = NULL, ground_generation = NULL
                 WHERE item_instance_id = $1 AND state = 'live' AND location_kind = 'ground'",
                &[&raw.as_slice()],
            )
            .map_err(map_sql)?;
        if updated != 1 {
            Err(PersistError::integrity(
                db_path(),
                "ground item was not unstamped",
            ))
        } else {
            Ok(())
        }
    }

    pub(crate) fn create_character(
        &mut self,
        login: &DevLogin,
        name: &str,
    ) -> Result<CharacterRosterEntry, PersistError> {
        create_character(&mut self.client, login, name)
    }

    pub(crate) fn provision_dev_user(&mut self, login: &DevLogin) -> Result<bool, PersistError> {
        provision_dev_user(&mut self.client, login)
    }

    pub(crate) fn user_registered(&mut self, login: &DevLogin) -> Result<bool, PersistError> {
        user_registered(&mut self.client, login)
    }

    pub(crate) fn resolve_existing(
        &mut self,
        login: &DevLogin,
    ) -> Result<PersistentCharacter, PersistError> {
        if !user_registered(&mut self.client, login)? {
            return Err(PersistError::CreateRejected(
                crate::error::CreateCharacterRejection::Unregistered,
            ));
        }
        match lookup(&mut self.client, login)? {
            Some(id) => load_character(&mut self.client, id),
            None => Err(PersistError::storage(
                "development user has no character to resolve",
            )),
        }
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

pub fn drop_test_schema(settings: &PostgresSettings) -> Result<(), PersistError> {
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

pub(crate) fn connect_url(url: &str) -> Result<Client, PersistError> {
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

pub(crate) fn database_name(url: &str) -> Result<String, PersistError> {
    let config =
        Config::from_str(url).map_err(|_| PersistError::storage("invalid database url"))?;
    config
        .get_dbname()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .ok_or_else(|| PersistError::storage("database url is missing a database name"))
}

pub(crate) fn database_user(url: &str) -> Result<String, PersistError> {
    let config =
        Config::from_str(url).map_err(|_| PersistError::storage("invalid database url"))?;
    config
        .get_user()
        .filter(|user| !user.is_empty())
        .map(str::to_string)
        .ok_or_else(|| PersistError::storage("database url is missing a user"))
}

pub(crate) fn grant_runtime(
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
    // It cannot create or drop objects, and it cannot insert development users.
    // Fact clears are the only deletes.
    // Command retry locks the stored row with SELECT FOR UPDATE, which
    // requires UPDATE on durable_commands even though the row is not rewritten.
    let sql = format!(
        "GRANT USAGE ON SCHEMA {schema_sql} TO {user_sql};
         GRANT SELECT ON {schema_sql}.schema_migrations TO {user_sql};
         GRANT SELECT, INSERT, UPDATE ON {schema_sql}.durable_meta, {schema_sql}.characters, {schema_sql}.item_instances TO {user_sql};
         GRANT SELECT ON {schema_sql}.dev_users TO {user_sql};
         GRANT SELECT, INSERT ON {schema_sql}.character_npcs_met, {schema_sql}.character_dialogue_heard, {schema_sql}.character_learned_abilities TO {user_sql};
         GRANT SELECT, INSERT, UPDATE ON {schema_sql}.durable_commands TO {user_sql};
         GRANT SELECT, INSERT, UPDATE, DELETE ON {schema_sql}.character_facts TO {user_sql};
         GRANT SELECT, INSERT, UPDATE, DELETE ON {schema_sql}.character_leases, {schema_sql}.channel_generations TO {user_sql};
         GRANT SELECT, INSERT ON {schema_sql}.item_id_reservations TO {user_sql};"
    );
    client.batch_execute(&sql).map_err(map_sql)
}

pub(crate) fn validated_schema(name: &str) -> Result<(), PersistError> {
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

pub(crate) fn prepare_schema(
    client: &mut Client,
    schema: &str,
    create: bool,
) -> Result<(), PersistError> {
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

pub(crate) fn migrate(client: &mut Client) -> Result<(), PersistError> {
    let present: bool = client
        .query_one("SELECT to_regclass('schema_migrations') IS NOT NULL", &[])
        .map_err(map_sql)?
        .get(0);
    if !present {
        let mut tx = client.transaction().map_err(map_sql)?;
        for (version, body) in MIGRATIONS {
            tx.batch_execute(body).map_err(map_sql)?;
            tx.execute(
                "INSERT INTO schema_migrations (version, body) VALUES ($1, $2)",
                &[version, body],
            )
            .map_err(map_sql)?;
        }
        return tx.commit().map_err(map_sql);
    }
    let rows = client
        .query(
            "SELECT version, body FROM schema_migrations ORDER BY version",
            &[],
        )
        .map_err(map_sql)?;
    if rows.len() > MIGRATIONS.len() {
        return Err(PersistError::migration(
            db_path(),
            "unexpected postgresql migration history",
        ));
    }
    for (index, row) in rows.iter().enumerate() {
        let version: i32 = row.get(0);
        let body: String = row.get(1);
        let Some((expected_version, expected_body)) = MIGRATIONS.get(index) else {
            return Err(PersistError::migration(
                db_path(),
                "unexpected postgresql migration history",
            ));
        };
        if version != *expected_version || body != *expected_body {
            return Err(PersistError::migration(
                db_path(),
                "applied postgresql migration does not match this build",
            ));
        }
    }
    if rows.len() == MIGRATIONS.len() {
        return Ok(());
    }
    let mut tx = client.transaction().map_err(map_sql)?;
    for (version, body) in MIGRATIONS.iter().skip(rows.len()) {
        tx.batch_execute(body).map_err(map_sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, body) VALUES ($1, $2)",
            &[version, body],
        )
        .map_err(map_sql)?;
    }
    tx.commit().map_err(map_sql)
}

/// Install migrations 1–3 only, so a test can prove migration 4 adds columns
/// without deleting rows. Does not connect to a reserved database name.
#[cfg(test)]
pub(crate) fn install_pre_health_schema(
    settings: &PostgresSettings,
) -> Result<Client, PersistError> {
    settings.refuse_reserved_database()?;
    let mut client = connect_url(&settings.url)?;
    prepare_schema(&mut client, &settings.schema, true)?;
    let mut tx = client.transaction().map_err(map_sql)?;
    for (version, body) in MIGRATIONS.iter().take(3) {
        tx.batch_execute(body).map_err(map_sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, body) VALUES ($1, $2)",
            &[version, body],
        )
        .map_err(map_sql)?;
    }
    tx.commit().map_err(map_sql)?;
    Ok(client)
}

/// Normal server startup refuses a missing, extra, or rewritten migration.
/// It does not apply a pending tail.
pub(crate) fn require_exact_migrations(client: &mut Client) -> Result<(), PersistError> {
    if !migrations_present(client)? {
        return Err(PersistError::migration(
            db_path(),
            "unsupported postgresql migration history",
        ));
    }
    let rows = client
        .query(
            "SELECT version, body FROM schema_migrations ORDER BY version",
            &[],
        )
        .map_err(map_sql)?;
    if rows.len() != MIGRATIONS.len() {
        return Err(PersistError::migration(
            db_path(),
            "unsupported postgresql migration history",
        ));
    }
    for (index, row) in rows.iter().enumerate() {
        let version: i32 = row.get(0);
        let body: String = row.get(1);
        let (expected_version, expected_body) = MIGRATIONS[index];
        if version != expected_version || body != expected_body {
            return Err(PersistError::migration(
                db_path(),
                "unsupported postgresql migration history",
            ));
        }
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

fn store_from_client(client: Client, settings: &PostgresSettings) -> PostgresStore {
    PostgresStore {
        client,
        runtime_url: settings.url.clone(),
        schema: settings.schema.clone(),
        rules: DurableContentRules::new(),
        #[cfg(test)]
        hide_commit_reply: false,
        #[cfg(test)]
        discard_connection_after_commit: false,
        connection_closed: false,
        #[cfg(test)]
        reconnect_failures_remaining: 0,
        channels: std::collections::BTreeMap::new(),
        #[cfg(test)]
        lease_barrier: None,
    }
}

pub(crate) fn validate_deployment_id(id: &str) -> Result<(), PersistError> {
    let ok = (1..=64).contains(&id.len())
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.');
    if ok {
        Ok(())
    } else {
        Err(PersistError::storage(
            "PURGATORY_DEPLOYMENT_ID must be 1 to 64 ASCII letters, digits, dots, underscores, or hyphens",
        ))
    }
}

/// `create` is the deliberate bootstrap. Normal open refuses a missing schema
/// instead of creating an empty world.
fn connect_prepared(settings: &PostgresSettings, create: bool) -> Result<Client, PersistError> {
    validated_schema(&settings.schema)?;
    validate_deployment_id(&settings.deployment_id)?;
    if create {
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
            bootstrap_empty(&mut migrator, &settings.deployment_id)?;
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
        return Ok(client);
    }
    // Normal open verifies the stored history. It does not create a schema,
    // apply a migration, or grant privileges.
    let mut client = connect_url(&settings.url)?;
    if !schema_exists(&mut client, &settings.schema)? {
        return Err(PersistError::migration(
            db_path(),
            "database schema is not bootstrapped",
        ));
    }
    prepare_schema(&mut client, &settings.schema, false)?;
    require_exact_migrations(&mut client)?;
    Ok(client)
}

fn schema_exists(client: &mut Client, schema: &str) -> Result<bool, PersistError> {
    let exists: bool = client
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM information_schema.schemata WHERE schema_name = $1)",
            &[&schema],
        )
        .map_err(map_sql)?
        .get(0);
    Ok(exists)
}

fn migrations_present(client: &mut Client) -> Result<bool, PersistError> {
    let present: bool = client
        .query_one("SELECT to_regclass('schema_migrations') IS NOT NULL", &[])
        .map_err(map_sql)?
        .get(0);
    Ok(present)
}

fn require_deployment(client: &mut Client, deployment_id: &str) -> Result<(), PersistError> {
    let row = client
        .query_opt(
            "SELECT value FROM durable_meta WHERE key = 'deployment_id'",
            &[],
        )
        .map_err(map_sql)?;
    let Some(row) = row else {
        return Err(PersistError::migration(
            db_path(),
            "database is not bootstrapped",
        ));
    };
    let stored: String = row.get(0);
    if stored != deployment_id {
        return Err(PersistError::migration(
            db_path(),
            "database deployment identity does not match",
        ));
    }
    Ok(())
}

pub(crate) fn bootstrap_empty(
    client: &mut Client,
    deployment_id: &str,
) -> Result<(), PersistError> {
    let mut tx = client.transaction().map_err(map_sql)?;
    let result = (|| {
        tx.execute("SELECT pg_advisory_xact_lock($1)", &[&IMPORT_LOCK_KEY])
            .map_err(map_sql)?;
        let existing = tx
            .query_opt(
                "SELECT value FROM durable_meta WHERE key = 'deployment_id'",
                &[],
            )
            .map_err(map_sql)?;
        if existing.is_some() {
            return Err(PersistError::conflict(
                db_path(),
                "database is already bootstrapped",
            ));
        }
        let characters: i64 = tx
            .query_one("SELECT count(*)::bigint FROM characters", &[])
            .map_err(map_sql)?
            .get(0);
        let items: i64 = tx
            .query_one("SELECT count(*)::bigint FROM item_instances", &[])
            .map_err(map_sql)?
            .get(0);
        let cutover = tx
            .query_opt("SELECT value FROM durable_meta WHERE key = 'cutover'", &[])
            .map_err(map_sql)?;
        if characters != 0 || items != 0 || cutover.is_some() {
            return Err(PersistError::conflict(
                db_path(),
                "database is not an empty application schema",
            ));
        }
        tx.execute(
            "INSERT INTO durable_meta (key, value) VALUES
                ('next_character_id', '1'),
                ('next_item_instance_id', '1'),
                ('deployment_id', $1),
                ('cutover', 'fresh')",
            &[&deployment_id],
        )
        .map_err(map_sql)?;
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
            persistence_revision, restore_revision, restore_map_authored, restore_point_id,
            restore_checkpoint_id, instance_exit_reason
        ) VALUES ($1, $2, $3, $4, $5, $6, $6, $7, $8, $9, $10)",
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

fn lease_insert_raced(err: &PersistError) -> bool {
    matches!(err, PersistError::Conflict { reason, .. } if reason.contains("character_leases"))
}

fn commit_once(
    client: &mut Client,
    command: &DurableCommand,
    request: &str,
    rules: &DurableContentRules,
    lease: Option<&LeaseAuthority>,
    channels: &std::collections::BTreeMap<i64, u64>,
    barrier: &mut Option<LeaseBarrier>,
) -> Result<DurableCommandResult, PersistError> {
    let mut tx = client.transaction().map_err(map_sql)?;
    let result = apply_command(&mut tx, command, request, rules, lease, channels, barrier);
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
    lease: Option<&LeaseAuthority>,
    channels: &std::collections::BTreeMap<i64, u64>,
    barrier: &mut Option<LeaseBarrier>,
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
    enforce_command_lease(tx, &locked, lease, barrier)?;
    let needs_ground = command
        .moves
        .iter()
        .any(|item| matches!(item.to, LiveDestination::Ground));
    let channel = if needs_ground || !command.reserved_uses.is_empty() {
        Some(require_live_channel(channels)?)
    } else {
        None
    };
    if let Some((channel_id, generation)) = channel {
        lifecycle::lock_live_channel(tx, channel_id, generation)?;
    }
    let ground = if needs_ground { channel } else { None };
    let reserved_ceiling = if command.reserved_uses.is_empty() {
        None
    } else {
        Some(read_next_item_id(tx)?)
    };
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
        retire_item(tx, *id, channels)?;
    }
    for item in &command.moves {
        if matches!(item.to, LiveDestination::Character { .. }) {
            move_item(
                tx,
                item.item_instance_id,
                LiveDestination::Ground,
                rules,
                None,
            )?;
        }
    }
    let mut minted = place_items(tx, command)?;
    if let Some(ceiling) = reserved_ceiling {
        let (channel_id, generation) = channel.ok_or_else(|| {
            PersistError::conflict(
                db_path(),
                "item id reservation has no live channel generation",
            )
        })?;
        minted.extend(apply_reserved_uses(
            tx, command, channel_id, generation, ceiling,
        )?);
    }
    for item in &command.moves {
        let stamp = match item.to {
            LiveDestination::Ground => ground,
            LiveDestination::Character { .. } => None,
        };
        move_item(tx, item.item_instance_id, item.to, rules, stamp)?;
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

fn read_next_item_id(tx: &mut postgres::Transaction<'_>) -> Result<u64, PersistError> {
    let row = tx
        .query_one(
            "SELECT value FROM durable_meta WHERE key = 'next_item_instance_id'",
            &[],
        )
        .map_err(map_sql)?;
    let text: String = row.get(0);
    text.parse::<u64>()
        .map_err(|_| PersistError::integrity(db_path(), "next_item_instance_id is corrupt"))
}

/// Insert ids that were reserved before they became visible. The counter is
/// not advanced here. The id must fall inside a range recorded for this
/// channel generation. An id below the counter with no such row was not issued
/// to this generation.
fn apply_reserved_uses(
    tx: &mut postgres::Transaction<'_>,
    command: &DurableCommand,
    channel_id: i64,
    generation: u64,
    ceiling: u64,
) -> Result<Vec<ItemInstanceId>, PersistError> {
    let generation_sql = revision_i64(generation)?;
    let mut written = Vec::new();
    for reserved in &command.reserved_uses {
        let raw_id = reserved.item_instance_id.raw();
        if raw_id == 0 || raw_id >= ceiling {
            return Err(PersistError::conflict(
                db_path(),
                format!("item {raw_id} was not reserved"),
            ));
        }
        let id_sql = item_id_i64(raw_id).map_err(|_| {
            PersistError::conflict(db_path(), format!("item {raw_id} was not reserved"))
        })?;
        let covered: bool = tx
            .query_one(
                "SELECT EXISTS (
                    SELECT 1 FROM item_id_reservations
                    WHERE channel_id = $1
                      AND generation = $2
                      AND range_start <= $3
                      AND range_end > $3
                )",
                &[&channel_id, &generation_sql, &id_sql],
            )
            .map_err(map_sql)?
            .get(0);
        if !covered {
            return Err(PersistError::conflict(
                db_path(),
                format!("item {raw_id} was not reserved for this channel generation"),
            ));
        }
        let raw = id_bytes(raw_id);
        let existing = tx
            .query_opt(
                "SELECT state FROM item_instances WHERE item_instance_id = $1 FOR UPDATE",
                &[&raw.as_slice()],
            )
            .map_err(map_sql)?;
        if let Some(row) = existing {
            let state: String = row.get(0);
            if state == "retired" {
                return Err(PersistError::conflict(
                    db_path(),
                    format!("item {raw_id} is retired and cannot be reused"),
                ));
            }
            return Err(PersistError::conflict(
                db_path(),
                format!("item {raw_id} already exists"),
            ));
        }
        let definition = content_i32(reserved.definition_content_id)?;
        let quantity = i32::try_from(reserved.quantity).map_err(|_| {
            PersistError::content(db_path(), "item quantity exceeds signed 32-bit storage")
        })?;
        match reserved.outcome {
            ReservedItemOutcome::Inventory { owner, slot } => {
                let owner_raw = id_bytes(owner.raw());
                let inventory = i16::try_from(slot).map_err(|_| {
                    PersistError::corrupt(db_path(), "inventory slot does not fit storage")
                })?;
                tx.execute(
                    "INSERT INTO item_instances (
                        item_instance_id, definition_content_id, quantity, state,
                        owner_character_id, location_kind, inventory_slot, equipment_slot
                    ) VALUES ($1, $2, $3, 'live', $4, 'inventory', $5, NULL)",
                    &[
                        &raw.as_slice() as &(dyn ToSql + Sync),
                        &definition,
                        &quantity,
                        &owner_raw.as_slice(),
                        &inventory,
                    ],
                )
                .map_err(map_sql)?;
            }
            ReservedItemOutcome::Retired => {
                tx.execute(
                    "INSERT INTO item_instances (
                        item_instance_id, definition_content_id, quantity, state,
                        owner_character_id, location_kind, inventory_slot, equipment_slot
                    ) VALUES ($1, $2, $3, 'retired', NULL, NULL, NULL, NULL)",
                    &[
                        &raw.as_slice() as &(dyn ToSql + Sync),
                        &definition,
                        &quantity,
                    ],
                )
                .map_err(map_sql)?;
            }
        }
        written.push(reserved.item_instance_id);
    }
    Ok(written)
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

fn reservation_channel(
    channels: &std::collections::BTreeMap<i64, u64>,
) -> Result<(i64, u64), PersistError> {
    match channels.len() {
        1 => {
            let (channel_id, generation) = channels.iter().next().expect("one channel");
            Ok((*channel_id, *generation))
        }
        0 => Err(PersistError::conflict(
            db_path(),
            "item id reservation has no live channel generation",
        )),
        _ => Err(PersistError::conflict(
            db_path(),
            "item id reservation channel is ambiguous",
        )),
    }
}

fn require_live_channel(
    channels: &std::collections::BTreeMap<i64, u64>,
) -> Result<(i64, u64), PersistError> {
    match channels.len() {
        1 => {
            let (channel_id, generation) = channels.iter().next().expect("one channel");
            Ok((*channel_id, *generation))
        }
        0 => Err(PersistError::conflict(
            db_path(),
            "ground write has no live channel generation",
        )),
        _ => Err(PersistError::conflict(
            db_path(),
            "ground channel is ambiguous",
        )),
    }
}

fn enforce_command_lease(
    tx: &mut postgres::Transaction<'_>,
    characters: &BTreeSet<CharacterId>,
    lease: Option<&LeaseAuthority>,
    barrier: &mut Option<LeaseBarrier>,
) -> Result<(), PersistError> {
    let mut owners = BTreeSet::new();
    for id in characters {
        let raw = id_bytes(id.raw());
        let owner: String = tx
            .query_one(
                "SELECT owner_login FROM characters WHERE character_id = $1",
                &[&raw.as_slice()],
            )
            .map_err(map_sql)?
            .get(0);
        let leased: bool = tx
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM character_leases WHERE owner_login = $1)",
                &[&owner],
            )
            .map_err(map_sql)?
            .get(0);
        if leased {
            owners.insert(owner);
        }
    }
    match (owners.len(), lease) {
        (0, None) => Ok(()),
        (1, Some(authority)) => {
            let owner = owners.iter().next().expect("one leased owner");
            if owner != authority.login.as_str() || !characters.contains(&authority.character_id) {
                return Err(PersistError::LeaseLost);
            }
            lifecycle::assert_live_lease(tx, authority, barrier)
        }
        _ => Err(PersistError::LeaseLost),
    }
}

fn move_item(
    tx: &mut postgres::Transaction<'_>,
    id: ItemInstanceId,
    to: LiveDestination,
    rules: &DurableContentRules,
    ground: Option<(i64, u64)>,
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
    let (ground_channel, ground_generation) = match to {
        LiveDestination::Ground => match ground {
            Some((channel_id, generation)) => (Some(channel_id), Some(revision_i64(generation)?)),
            None => (None, None),
        },
        LiveDestination::Character { .. } => (None, None),
    };
    let updated = tx
        .execute(
            "UPDATE item_instances
             SET owner_character_id = $2, location_kind = $3, inventory_slot = $4, equipment_slot = $5,
                 ground_channel_id = $6, ground_generation = $7
             WHERE item_instance_id = $1 AND state = 'live'",
            &[
                &raw.as_slice() as &(dyn ToSql + Sync),
                &owner_param,
                &kind,
                &inventory,
                &equipment,
                &ground_channel,
                &ground_generation,
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

fn retire_item(
    tx: &mut postgres::Transaction<'_>,
    id: ItemInstanceId,
    channels: &std::collections::BTreeMap<i64, u64>,
) -> Result<(), PersistError> {
    let raw = id_bytes(id.raw());
    let row = tx
        .query_opt(
            "SELECT state, location_kind, ground_channel_id, ground_generation
             FROM item_instances WHERE item_instance_id = $1 FOR UPDATE",
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
    let kind: Option<String> = row.get(1);
    if kind.as_deref() == Some("ground") {
        let (channel_id, generation) = match channels.len() {
            1 => {
                let (channel_id, generation) = channels.iter().next().expect("one channel");
                (*channel_id, *generation)
            }
            0 => {
                return Err(PersistError::conflict(
                    db_path(),
                    "ground retire has no live channel generation",
                ));
            }
            _ => {
                return Err(PersistError::conflict(
                    db_path(),
                    "ground channel is ambiguous",
                ));
            }
        };
        lifecycle::lock_live_channel(tx, channel_id, generation)?;
        let generation = revision_i64(generation)?;
        let updated = tx
            .execute(
                "UPDATE item_instances
                 SET state = 'retired', owner_character_id = NULL, location_kind = NULL,
                     inventory_slot = NULL, equipment_slot = NULL,
                     ground_channel_id = NULL, ground_generation = NULL
                 WHERE item_instance_id = $1 AND state = 'live' AND location_kind = 'ground'
                   AND ground_channel_id = $2 AND ground_generation = $3",
                &[
                    &raw.as_slice() as &(dyn ToSql + Sync),
                    &channel_id,
                    &generation,
                ],
            )
            .map_err(map_sql)?;
        if updated != 1 {
            return Err(PersistError::conflict(
                db_path(),
                format!("ground item {} could not be retired", id.raw()),
            ));
        }
        return Ok(());
    }
    let updated = tx
        .execute(
            "UPDATE item_instances
             SET state = 'retired', owner_character_id = NULL, location_kind = NULL,
                 inventory_slot = NULL, equipment_slot = NULL,
                 ground_channel_id = NULL, ground_generation = NULL
             WHERE item_instance_id = $1 AND state = 'live'
               AND location_kind IS DISTINCT FROM 'ground'
               AND owner_character_id IS NOT NULL",
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

/// The server has acknowledged `COMMIT`. This connection is then made unusable
/// before the caller can observe that reply, and the follow-up read must fail.
#[cfg(test)]
fn recover_on_unusable_connection(
    client: &mut Client,
    connection_closed: &mut bool,
) -> Result<DurableCommandResult, PersistError> {
    let _ = client.batch_execute("SELECT pg_terminate_backend(pg_backend_pid())");
    let read = client.query_opt("SELECT 1", &[]).map_err(map_sql);
    *connection_closed = true;
    match read {
        Err(err) => Err(PersistError::storage(format!(
            "commit outcome unknown: {err}"
        ))),
        Ok(_) => Err(PersistError::storage(
            "commit outcome unknown: the database connection closed before the commit reply could be read",
        )),
    }
}

fn save_restore(
    client: &mut Client,
    snapshot: PersistentCharacterSnapshot,
    lease: Option<&LeaseAuthority>,
    barrier: &mut Option<LeaseBarrier>,
) -> Result<(), PersistError> {
    let mut tx = client.transaction().map_err(map_sql)?;
    let result = (|| {
        let raw = id_bytes(snapshot.character_id.raw());
        let row = tx
            .query_opt(
                "SELECT persistence_revision, restore_revision, restore_map_authored,
                        restore_point_id, restore_checkpoint_id, instance_exit_reason,
                        current_health, health_revision
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
        let recorded: i64 = row.get(1);
        let recorded = u64::try_from(recorded).map_err(|_| {
            PersistError::integrity(db_path(), "stored restore revision is negative")
        })?;
        let stored_map: String = row.get(2);
        let stored_point: String = row.get(3);
        let stored_checkpoint: Option<String> = row.get(4);
        let stored_exit: Option<String> = row.get(5);
        let stored_health_revision: i64 = row.get(7);
        let stored_health_revision = u64::try_from(stored_health_revision).map_err(|_| {
            PersistError::integrity(db_path(), "stored health revision is negative")
        })?;
        if snapshot.health_revision > stored_health_revision
            && let Some(milli) = snapshot.current_health_milli
        {
            write_health(&mut tx, &raw, milli, snapshot.health_revision)?;
        }
        let owner: String = tx
            .query_one(
                "SELECT owner_login FROM characters WHERE character_id = $1",
                &[&raw.as_slice()],
            )
            .map_err(map_sql)?
            .get(0);
        let leased: bool = tx
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM character_leases WHERE owner_login = $1)",
                &[&owner],
            )
            .map_err(map_sql)?
            .get(0);
        if leased {
            let Some(authority) = lease else {
                return Err(PersistError::LeaseLost);
            };
            if authority.login.as_str() != owner || authority.character_id != snapshot.character_id
            {
                return Err(PersistError::LeaseLost);
            }
            lifecycle::assert_live_lease(&mut tx, authority, barrier)?;
        } else if lease.is_some() {
            return Err(PersistError::LeaseLost);
        }
        let same_restore = stored_map == snapshot.restore.map_authored
            && stored_point == snapshot.restore.point_id
            && stored_checkpoint.as_deref() == snapshot.restore.checkpoint_id.as_deref()
            && stored_exit.as_deref()
                == snapshot
                    .instance_exit
                    .as_ref()
                    .and_then(|exit| exit.reason.as_deref());
        // A durable command advances persistence_revision and leaves
        // restore_revision behind. The first snapshot at the new revision
        // records the restore. A different snapshot at that same revision is
        // stale, including one that arrives later.
        if snapshot.persistence_revision < current {
            return Ok(());
        }
        if snapshot.persistence_revision == current {
            if recorded == current && same_restore {
                return Ok(());
            }
            if recorded < current {
                let revision = revision_i64(current)?;
                write_restore(&mut tx, &raw, None, revision, &snapshot)?;
            }
            return Ok(());
        }
        let revision = revision_i64(snapshot.persistence_revision)?;
        write_restore(&mut tx, &raw, Some(revision), revision, &snapshot)?;
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

fn write_health(
    tx: &mut postgres::Transaction<'_>,
    raw: &[u8; 8],
    milli: u32,
    health_revision: u64,
) -> Result<(), PersistError> {
    let current = f64::from(milli) / 1000.0;
    let revision = i64::try_from(health_revision)
        .map_err(|_| PersistError::integrity(db_path(), "health revision does not fit"))?;
    tx.execute(
        "UPDATE characters
         SET current_health = $2, health_revision = $3
         WHERE character_id = $1 AND health_revision < $3",
        &[&raw.as_slice() as &(dyn ToSql + Sync), &current, &revision],
    )
    .map_err(map_sql)?;
    Ok(())
}

fn write_restore(
    tx: &mut postgres::Transaction<'_>,
    raw: &[u8; 8],
    persistence_revision: Option<i64>,
    restore_revision: i64,
    snapshot: &PersistentCharacterSnapshot,
) -> Result<(), PersistError> {
    let checkpoint = snapshot.restore.checkpoint_id.as_deref();
    let exit_reason = snapshot
        .instance_exit
        .as_ref()
        .and_then(|exit| exit.reason.as_deref());
    let updated = if let Some(revision) = persistence_revision {
        tx.execute(
            "UPDATE characters
             SET persistence_revision = $2, restore_revision = $3, restore_map_authored = $4,
                 restore_point_id = $5, restore_checkpoint_id = $6, instance_exit_reason = $7
             WHERE character_id = $1",
            &[
                &raw.as_slice() as &(dyn ToSql + Sync),
                &revision,
                &restore_revision,
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
             SET restore_revision = $2, restore_map_authored = $3, restore_point_id = $4,
                 restore_checkpoint_id = $5, instance_exit_reason = $6
             WHERE character_id = $1",
            &[
                &raw.as_slice() as &(dyn ToSql + Sync),
                &restore_revision,
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
        let registered: bool = tx
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM dev_users WHERE login = $1)",
                &[&login.as_str()],
            )
            .map_err(map_sql)?
            .get(0);
        if !registered {
            return Err(PersistError::CreateRejected(
                crate::error::CreateCharacterRejection::Unregistered,
            ));
        }
        let entry = CharacterRosterEntry {
            character_id: CharacterId::from_raw(raw),
            display_name,
        };
        let character = PersistentCharacter::new_default(entry.character_id);
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

fn provision_dev_user(client: &mut Client, login: &DevLogin) -> Result<bool, PersistError> {
    let inserted = client
        .execute(
            "INSERT INTO dev_users (login) VALUES ($1) ON CONFLICT DO NOTHING",
            &[&login.as_str()],
        )
        .map_err(map_sql)?;
    Ok(inserted == 1)
}

fn user_registered(client: &mut Client, login: &DevLogin) -> Result<bool, PersistError> {
    let registered: bool = client
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM dev_users WHERE login = $1)",
            &[&login.as_str()],
        )
        .map_err(map_sql)?
        .get(0);
    Ok(registered)
}

fn roster(
    client: &mut Client,
    login: &DevLogin,
) -> Result<Vec<CharacterRosterEntry>, PersistError> {
    if !user_registered(client, login)? {
        return Err(PersistError::CreateRejected(
            crate::error::CreateCharacterRejection::Unregistered,
        ));
    }
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

fn load_character(
    client: &mut Client,
    id: CharacterId,
) -> Result<PersistentCharacter, PersistError> {
    let raw = id_bytes(id.raw());
    let row = client
        .query_opt(
            "SELECT persistence_revision, restore_map_authored, restore_point_id,
                    restore_checkpoint_id, instance_exit_reason, current_health, health_revision
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
    let (current_health_milli, health_revision) = read_health(&row, 5, 6)?;
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
        current_health_milli,
        health_revision,
    })
}

pub(crate) fn read_health(
    row: &postgres::Row,
    health_index: usize,
    revision_index: usize,
) -> Result<(Option<u32>, u64), PersistError> {
    let current: Option<f64> = row.get(health_index);
    let revision: i64 = row.get(revision_index);
    let health_revision = u64::try_from(revision)
        .map_err(|_| PersistError::integrity(db_path(), "stored health revision is negative"))?;
    let current_health_milli = current.map(|value| {
        if !value.is_finite() || value <= 0.0 {
            return 0;
        }
        let milli = (value * 1000.0).round();
        if milli >= f64::from(u32::MAX) {
            u32::MAX
        } else {
            milli as u32
        }
    });
    Ok((current_health_milli, health_revision))
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

fn item_id_i64(value: u64) -> Result<i64, PersistError> {
    i64::try_from(value).map_err(|_| PersistError::ItemIdsExhausted)
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

pub(crate) fn map_sql_pub(err: postgres::Error) -> PersistError {
    map_sql(err)
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
    let mut text = err.to_string();
    let mut source = std::error::Error::source(&err);
    while let Some(inner) = source {
        text.push_str(": ");
        text.push_str(&inner.to_string());
        source = inner.source();
    }
    PersistError::storage(sanitize(&text))
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
        reserved_uses: command
            .reserved_uses
            .iter()
            .map(|reserved| CanonReserved {
                item_instance_id: reserved.item_instance_id.raw(),
                definition: reserved.definition_content_id.token(),
                quantity: reserved.quantity,
                outcome: match reserved.outcome {
                    ReservedItemOutcome::Inventory { owner, slot } => {
                        CanonReservedOutcome::Inventory {
                            owner: owner.raw(),
                            slot,
                        }
                    }
                    ReservedItemOutcome::Retired => CanonReservedOutcome::Retired,
                },
            })
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    reserved_uses: Vec<CanonReserved>,
}

#[derive(Serialize)]
struct CanonReserved {
    item_instance_id: u64,
    definition: u64,
    quantity: u32,
    outcome: CanonReservedOutcome,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CanonReservedOutcome {
    Inventory { owner: u64, slot: u16 },
    Retired,
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
