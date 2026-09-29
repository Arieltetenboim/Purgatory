//! Phase 12B character leases and channel generations.
//!
//! Lease expiry uses `clock_timestamp()` after the row lock. `now()` is fixed
//! at transaction start and would treat a lease as live after the wait.
//! Character-lease generations and channel generations are different rows.

use std::time::Duration;

use postgres::Transaction;
use purgatory_common::{CharacterId, ContentId, DevLogin, ItemInstanceId};

use crate::character::{PERSISTENCE_SCHEMA_VERSION, PersistentCharacter};
use crate::domain::{
    CharacterItemLocation, CharacterNarrativeState, DurableEquipmentSlot, ItemOwner, ItemRecord,
    db_path,
};
use crate::error::PersistError;

/// How long a character lease stays authoritative without renewal.
pub const CHARACTER_LEASE_EXPIRY: Duration = Duration::from_secs(60);
/// How often the holding process renews a character lease. This is shorter
/// than the expiry so one missed renewal does not drop a live session.
pub const CHARACTER_LEASE_RENEWAL: Duration = Duration::from_secs(10);
/// How long a channel generation stays live without renewal. Distinct from
/// [`CHARACTER_LEASE_EXPIRY`] even though the duration matches.
pub const CHANNEL_GENERATION_EXPIRY: Duration = Duration::from_secs(60);
/// How often the holding process renews its channel generation.
pub const CHANNEL_GENERATION_RENEWAL: Duration = Duration::from_secs(10);

/// Test hook. The transaction sends on `entered` after it locks the row and
/// before it reads `clock_timestamp()`, then waits for `release`.
#[derive(Debug)]
pub struct LeaseBarrier {
    pub entered: std::sync::mpsc::SyncSender<()>,
    pub release: std::sync::mpsc::Receiver<()>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaseAuthority {
    pub login: DevLogin,
    pub character_id: CharacterId,
    pub generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedRestore {
    pub character: PersistentCharacter,
    pub items: Vec<ItemRecord>,
    pub narrative: CharacterNarrativeState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Admission {
    Granted {
        authority: LeaseAuthority,
        restore: Box<OwnedRestore>,
    },
    /// An unexpired lease is held and this call did not replace it.
    Held,
    NotOwned,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChannelClaim {
    Claimed {
        channel_id: i64,
        generation: u64,
        retired_ground: u64,
    },
    /// Another process still heartbeats this channel. Do not sweep its drops.
    Busy { channel_id: i64, generation: u64 },
}

fn expiry_secs(duration: Duration) -> Result<i32, PersistError> {
    i32::try_from(duration.as_secs())
        .map_err(|_| PersistError::integrity(db_path(), "lease expiry does not fit an interval"))
}

fn pause(barrier: &mut Option<LeaseBarrier>) {
    let Some(barrier) = barrier.take() else {
        return;
    };
    let _ = barrier.entered.send(());
    let _ = barrier.release.recv();
}

pub(crate) fn admit(
    tx: &mut Transaction<'_>,
    login: &DevLogin,
    character_id: CharacterId,
    barrier: &mut Option<LeaseBarrier>,
) -> Result<Admission, PersistError> {
    if !lock_owned(tx, login, character_id)? {
        return Ok(Admission::NotOwned);
    }
    let secs = expiry_secs(CHARACTER_LEASE_EXPIRY)?;
    let existing = lock_lease(tx, login)?;
    pause(barrier);
    let Some((leased_character, generation)) = existing else {
        insert_lease(tx, login, character_id, 1, secs)?;
        let restore = load_restore(tx, character_id)?;
        return Ok(Admission::Granted {
            authority: LeaseAuthority {
                login: login.clone(),
                character_id,
                generation: 1,
            },
            restore: Box::new(restore),
        });
    };
    if !lease_expired(tx, login)? {
        // Same character or a different one: a live holder is not replaced.
        // Same-process reconnect uses [`supersede`] after World has stopped.
        let _ = (leased_character, generation);
        return Ok(Admission::Held);
    }
    let next = generation.checked_add(1).ok_or_else(|| {
        PersistError::integrity(db_path(), "character lease generation exhausted")
    })?;
    let next_i = revision_i64(next)?;
    let raw = id_bytes(character_id.raw());
    let updated = tx
        .execute(
            "UPDATE character_leases
             SET character_id = $2, generation = $3,
                 expires_at = clock_timestamp() + make_interval(secs => $4)
             WHERE owner_login = $1 AND expires_at <= clock_timestamp()",
            &[&login.as_str(), &raw.as_slice(), &next_i, &secs],
        )
        .map_err(map_sql)?;
    if updated != 1 {
        return Err(PersistError::LeaseLost);
    }
    let restore = load_restore(tx, character_id)?;
    Ok(Admission::Granted {
        authority: LeaseAuthority {
            login: login.clone(),
            character_id,
            generation: next,
        },
        restore: Box::new(restore),
    })
}

pub(crate) fn supersede(
    tx: &mut Transaction<'_>,
    authority: &LeaseAuthority,
    barrier: &mut Option<LeaseBarrier>,
) -> Result<(LeaseAuthority, OwnedRestore), PersistError> {
    if !lock_owned(tx, &authority.login, authority.character_id)? {
        return Err(PersistError::LeaseLost);
    }
    let existing = lock_lease(tx, &authority.login)?;
    pause(barrier);
    let Some((character_id, generation)) = existing else {
        return Err(PersistError::LeaseLost);
    };
    if character_id != authority.character_id || generation != authority.generation {
        return Err(PersistError::LeaseLost);
    }
    if lease_expired(tx, &authority.login)? {
        return Err(PersistError::LeaseLost);
    }
    let next = generation.checked_add(1).ok_or_else(|| {
        PersistError::integrity(db_path(), "character lease generation exhausted")
    })?;
    let secs = expiry_secs(CHARACTER_LEASE_EXPIRY)?;
    let next_i = revision_i64(next)?;
    let updated = tx
        .execute(
            "UPDATE character_leases
             SET generation = $3,
                 expires_at = clock_timestamp() + make_interval(secs => $4)
             WHERE owner_login = $1 AND character_id = $2 AND generation = $5
               AND expires_at > clock_timestamp()",
            &[
                &authority.login.as_str(),
                &id_bytes(authority.character_id.raw()).as_slice(),
                &next_i,
                &secs,
                &revision_i64(generation)?,
            ],
        )
        .map_err(map_sql)?;
    if updated != 1 {
        return Err(PersistError::LeaseLost);
    }
    let restore = load_restore(tx, authority.character_id)?;
    Ok((
        LeaseAuthority {
            login: authority.login.clone(),
            character_id: authority.character_id,
            generation: next,
        },
        restore,
    ))
}

pub(crate) fn renew(
    tx: &mut Transaction<'_>,
    authority: &LeaseAuthority,
    barrier: &mut Option<LeaseBarrier>,
) -> Result<(), PersistError> {
    let _ = lock_lease(tx, &authority.login)?;
    pause(barrier);
    let secs = expiry_secs(CHARACTER_LEASE_EXPIRY)?;
    let updated = tx
        .execute(
            "UPDATE character_leases
             SET expires_at = clock_timestamp() + make_interval(secs => $4)
             WHERE owner_login = $1 AND character_id = $2 AND generation = $3
               AND expires_at > clock_timestamp()",
            &[
                &authority.login.as_str(),
                &id_bytes(authority.character_id.raw()).as_slice(),
                &revision_i64(authority.generation)?,
                &secs,
            ],
        )
        .map_err(map_sql)?;
    if updated != 1 {
        Err(PersistError::LeaseLost)
    } else {
        Ok(())
    }
}

pub(crate) fn release(
    tx: &mut Transaction<'_>,
    authority: &LeaseAuthority,
    barrier: &mut Option<LeaseBarrier>,
) -> Result<(), PersistError> {
    let _ = lock_lease(tx, &authority.login)?;
    pause(barrier);
    let deleted = tx
        .execute(
            "DELETE FROM character_leases
             WHERE owner_login = $1 AND character_id = $2 AND generation = $3",
            &[
                &authority.login.as_str(),
                &id_bytes(authority.character_id.raw()).as_slice(),
                &revision_i64(authority.generation)?,
            ],
        )
        .map_err(map_sql)?;
    if deleted != 1 {
        Err(PersistError::LeaseLost)
    } else {
        Ok(())
    }
}

/// The lease row is locked by the caller before this check. The comparison
/// uses `clock_timestamp()` so a lock wait cannot extend an expired lease.
pub(crate) fn assert_live_lease(
    tx: &mut Transaction<'_>,
    authority: &LeaseAuthority,
    barrier: &mut Option<LeaseBarrier>,
) -> Result<(), PersistError> {
    let existing = lock_lease(tx, &authority.login)?;
    pause(barrier);
    let Some((character_id, generation)) = existing else {
        return Err(PersistError::LeaseLost);
    };
    if character_id != authority.character_id || generation != authority.generation {
        return Err(PersistError::LeaseLost);
    }
    if lease_expired(tx, &authority.login)? {
        Err(PersistError::LeaseLost)
    } else {
        Ok(())
    }
}

pub(crate) fn claim_channel(
    tx: &mut Transaction<'_>,
    channel_id: i64,
    retire_limit: Option<i64>,
    barrier: &mut Option<LeaseBarrier>,
) -> Result<ChannelClaim, PersistError> {
    if channel_id < 0 {
        return Err(PersistError::conflict(db_path(), "channel id is negative"));
    }
    let row = tx
        .query_opt(
            "SELECT generation FROM channel_generations WHERE channel_id = $1 FOR UPDATE",
            &[&channel_id],
        )
        .map_err(map_sql)?;
    pause(barrier);
    let secs = expiry_secs(CHANNEL_GENERATION_EXPIRY)?;
    let generation = if let Some(row) = row {
        let current: i64 = row.get(0);
        let live: bool = tx
            .query_one(
                "SELECT expires_at > clock_timestamp() FROM channel_generations WHERE channel_id = $1",
                &[&channel_id],
            )
            .map_err(map_sql)?
            .get(0);
        if live {
            return Ok(ChannelClaim::Busy {
                channel_id,
                generation: u64::try_from(current).map_err(|_| {
                    PersistError::integrity(db_path(), "channel generation is negative")
                })?,
            });
        }
        let next = current
            .checked_add(1)
            .ok_or_else(|| PersistError::integrity(db_path(), "channel generation exhausted"))?;
        tx.execute(
            "UPDATE channel_generations
             SET generation = $2, expires_at = clock_timestamp() + make_interval(secs => $3)
             WHERE channel_id = $1 AND expires_at <= clock_timestamp()",
            &[&channel_id, &next, &secs],
        )
        .map_err(map_sql)?;
        next
    } else {
        tx.execute(
            "INSERT INTO channel_generations (channel_id, generation, expires_at)
             VALUES ($1, 1, clock_timestamp() + make_interval(secs => $2))",
            &[&channel_id, &secs],
        )
        .map_err(map_sql)?;
        1
    };
    let retired = retire_ground(tx, channel_id, generation, retire_limit)?;
    Ok(ChannelClaim::Claimed {
        channel_id,
        generation: u64::try_from(generation)
            .map_err(|_| PersistError::integrity(db_path(), "channel generation is negative"))?,
        retired_ground: retired,
    })
}

pub(crate) fn sweep_held_channel(
    tx: &mut Transaction<'_>,
    channel_id: i64,
    generation: u64,
    limit: Option<i64>,
) -> Result<u64, PersistError> {
    lock_live_channel(tx, channel_id, generation)?;
    retire_ground(tx, channel_id, revision_i64(generation)?, limit)
}

pub(crate) fn renew_channel(
    tx: &mut Transaction<'_>,
    channel_id: i64,
    generation: u64,
) -> Result<(), PersistError> {
    let secs = expiry_secs(CHANNEL_GENERATION_EXPIRY)?;
    let updated = tx
        .execute(
            "UPDATE channel_generations
             SET expires_at = clock_timestamp() + make_interval(secs => $3)
             WHERE channel_id = $1 AND generation = $2
               AND expires_at > clock_timestamp()",
            &[&channel_id, &revision_i64(generation)?, &secs],
        )
        .map_err(map_sql)?;
    if updated != 1 {
        Err(PersistError::LeaseLost)
    } else {
        Ok(())
    }
}

pub(crate) fn release_channel(
    tx: &mut Transaction<'_>,
    channel_id: i64,
    generation: u64,
) -> Result<(), PersistError> {
    let deleted = tx
        .execute(
            "DELETE FROM channel_generations WHERE channel_id = $1 AND generation = $2",
            &[&channel_id, &revision_i64(generation)?],
        )
        .map_err(map_sql)?;
    if deleted != 1 {
        Err(PersistError::LeaseLost)
    } else {
        Ok(())
    }
}

/// Retire ordinary ground rows for one channel generation. Rows owned by
/// another generation are left in place. Null-scoped rows are previous-run
/// drops and are retired only when no other channel generation is live.
pub(crate) fn retire_ground(
    tx: &mut Transaction<'_>,
    channel_id: i64,
    live_generation: i64,
    limit: Option<i64>,
) -> Result<u64, PersistError> {
    let cap = limit.unwrap_or(i64::MAX);
    let scoped = tx
        .execute(
            "UPDATE item_instances
             SET state = 'retired', owner_character_id = NULL, location_kind = NULL,
                 inventory_slot = NULL, equipment_slot = NULL,
                 ground_channel_id = NULL, ground_generation = NULL
             WHERE item_instance_id IN (
                 SELECT item_instance_id FROM item_instances
                 WHERE state = 'live' AND location_kind = 'ground'
                   AND ground_channel_id = $1
                   AND ground_generation IS DISTINCT FROM $2
                 ORDER BY item_instance_id
                 LIMIT $3
             )",
            &[&channel_id, &live_generation, &cap],
        )
        .map_err(map_sql)?;
    let scoped_i = i64::try_from(scoped).unwrap_or(i64::MAX);
    if scoped_i >= cap {
        return Ok(scoped);
    }
    let remaining = cap.saturating_sub(scoped_i);
    let unscoped = tx
        .execute(
            "UPDATE item_instances
             SET state = 'retired', owner_character_id = NULL, location_kind = NULL,
                 inventory_slot = NULL, equipment_slot = NULL,
                 ground_channel_id = NULL, ground_generation = NULL
             WHERE item_instance_id IN (
                 SELECT item_instance_id FROM item_instances
                 WHERE state = 'live' AND location_kind = 'ground'
                   AND ground_channel_id IS NULL
                   AND NOT EXISTS (
                       SELECT 1 FROM channel_generations
                       WHERE channel_id <> $1 AND expires_at > clock_timestamp()
                   )
                 ORDER BY item_instance_id
                 LIMIT $2
             )",
            &[&channel_id, &remaining],
        )
        .map_err(map_sql)?;
    Ok(scoped.saturating_add(unscoped))
}

pub(crate) fn lock_live_channel(
    tx: &mut Transaction<'_>,
    channel_id: i64,
    generation: u64,
) -> Result<(), PersistError> {
    let row = tx
        .query_opt(
            "SELECT generation FROM channel_generations WHERE channel_id = $1 FOR UPDATE",
            &[&channel_id],
        )
        .map_err(map_sql)?;
    let Some(row) = row else {
        return Err(PersistError::conflict(
            db_path(),
            "ground write has no live channel generation",
        ));
    };
    let current: i64 = row.get(0);
    if current != revision_i64(generation)? {
        return Err(PersistError::conflict(
            db_path(),
            "stale channel generation cannot write ground drops",
        ));
    }
    let live: bool = tx
        .query_one(
            "SELECT expires_at > clock_timestamp()
             FROM channel_generations WHERE channel_id = $1",
            &[&channel_id],
        )
        .map_err(map_sql)?
        .get(0);
    if live {
        Ok(())
    } else {
        Err(PersistError::conflict(
            db_path(),
            "expired channel generation cannot write ground drops",
        ))
    }
}

fn lock_owned(
    tx: &mut Transaction<'_>,
    login: &DevLogin,
    character_id: CharacterId,
) -> Result<bool, PersistError> {
    let raw = id_bytes(character_id.raw());
    let row = tx
        .query_opt(
            "SELECT owner_login FROM characters WHERE character_id = $1 FOR UPDATE",
            &[&raw.as_slice()],
        )
        .map_err(map_sql)?;
    let Some(row) = row else {
        return Ok(false);
    };
    let owner: String = row.get(0);
    Ok(owner == login.as_str())
}

fn lock_lease(
    tx: &mut Transaction<'_>,
    login: &DevLogin,
) -> Result<Option<(CharacterId, u64)>, PersistError> {
    let row = tx
        .query_opt(
            "SELECT character_id, generation FROM character_leases WHERE owner_login = $1 FOR UPDATE",
            &[&login.as_str()],
        )
        .map_err(map_sql)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let bytes: Vec<u8> = row.get(0);
    let generation: i64 = row.get(1);
    Ok(Some((
        CharacterId::from_raw(id_from_bytes(&bytes)?),
        u64::try_from(generation)
            .map_err(|_| PersistError::integrity(db_path(), "lease generation is negative"))?,
    )))
}

fn lease_expired(tx: &mut Transaction<'_>, login: &DevLogin) -> Result<bool, PersistError> {
    let expired: bool = tx
        .query_one(
            "SELECT expires_at <= clock_timestamp() FROM character_leases WHERE owner_login = $1",
            &[&login.as_str()],
        )
        .map_err(map_sql)?
        .get(0);
    Ok(expired)
}

fn insert_lease(
    tx: &mut Transaction<'_>,
    login: &DevLogin,
    character_id: CharacterId,
    generation: i64,
    secs: i32,
) -> Result<(), PersistError> {
    tx.execute(
        "INSERT INTO character_leases (owner_login, character_id, generation, expires_at)
         VALUES ($1, $2, $3, clock_timestamp() + make_interval(secs => $4))",
        &[
            &login.as_str(),
            &id_bytes(character_id.raw()).as_slice(),
            &generation,
            &secs,
        ],
    )
    .map_err(map_sql)?;
    Ok(())
}

fn load_restore(tx: &mut Transaction<'_>, id: CharacterId) -> Result<OwnedRestore, PersistError> {
    let raw = id_bytes(id.raw());
    let row = tx
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
    if map_authored.is_empty() || point_id.is_empty() {
        return Err(PersistError::corrupt(
            db_path(),
            "character restore is missing",
        ));
    }
    let character = PersistentCharacter {
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
    };
    let items = load_items(tx, &raw)?;
    let narrative = load_narrative(tx, &raw)?;
    Ok(OwnedRestore {
        character,
        items,
        narrative,
    })
}

fn load_items(tx: &mut Transaction<'_>, raw: &[u8; 8]) -> Result<Vec<ItemRecord>, PersistError> {
    let rows = tx
        .query(
            "SELECT item_instance_id, definition_content_id, quantity, location_kind,
                    inventory_slot, equipment_slot
             FROM item_instances
             WHERE owner_character_id = $1 AND state = 'live'
             ORDER BY item_instance_id",
            &[&raw.as_slice()],
        )
        .map_err(map_sql)?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let id_bytes: Vec<u8> = row.get(0);
        let definition: i32 = row.get(1);
        let quantity: i32 = row.get(2);
        let kind: Option<String> = row.get(3);
        let slot: Option<i16> = row.get(4);
        let equipment: Option<String> = row.get(5);
        let item_id = ItemInstanceId::from_raw(id_from_bytes(&id_bytes)?);
        let definition = u32::try_from(definition).map_err(|_| {
            PersistError::integrity(db_path(), "stored item definition is negative")
        })?;
        let quantity = u32::try_from(quantity)
            .map_err(|_| PersistError::integrity(db_path(), "stored item quantity is negative"))?;
        if quantity == 0 {
            return Err(PersistError::corrupt(
                db_path(),
                "live item quantity is zero",
            ));
        }
        let location = match kind.as_deref() {
            Some("inventory") => {
                let slot = slot.ok_or_else(|| {
                    PersistError::corrupt(db_path(), "inventory item has no slot")
                })?;
                CharacterItemLocation::Inventory {
                    slot: u16::try_from(slot).map_err(|_| {
                        PersistError::corrupt(db_path(), "inventory slot is negative")
                    })?,
                }
            }
            Some("equipped") => {
                let name = equipment
                    .ok_or_else(|| PersistError::corrupt(db_path(), "equipped item has no slot"))?;
                CharacterItemLocation::Equipped {
                    slot: DurableEquipmentSlot::parse(&name).ok_or_else(|| {
                        PersistError::corrupt(db_path(), "equipped item slot is unknown")
                    })?,
                }
            }
            _ => {
                return Err(PersistError::corrupt(
                    db_path(),
                    "owned item location is invalid",
                ));
            }
        };
        items.push(ItemRecord {
            item_instance_id: item_id,
            definition_content_id: ContentId::from_raw(definition),
            quantity,
            owner: ItemOwner::Character {
                character_id: CharacterId::from_raw(id_from_bytes(raw)?),
                location,
            },
        });
    }
    Ok(items)
}

fn load_narrative(
    tx: &mut Transaction<'_>,
    raw: &[u8; 8],
) -> Result<CharacterNarrativeState, PersistError> {
    let mut state = CharacterNarrativeState::default();
    for row in tx
        .query(
            "SELECT fact_key, value FROM character_facts WHERE character_id = $1 ORDER BY fact_key",
            &[&raw.as_slice()],
        )
        .map_err(map_sql)?
    {
        let key: String = row.get(0);
        if key.is_empty() {
            return Err(PersistError::corrupt(db_path(), "fact key is empty"));
        }
        state.facts.insert(key, row.get(1));
    }
    for row in tx
        .query(
            "SELECT npc_authored FROM character_npcs_met WHERE character_id = $1 ORDER BY npc_authored",
            &[&raw.as_slice()],
        )
        .map_err(map_sql)?
    {
        state.npcs_met.insert(row.get(0));
    }
    for row in tx
        .query(
            "SELECT npc_content_id, beat_id FROM character_dialogue_heard
             WHERE character_id = $1 ORDER BY npc_content_id, beat_id",
            &[&raw.as_slice()],
        )
        .map_err(map_sql)?
    {
        let npc: i32 = row.get(0);
        let beat: String = row.get(1);
        if beat.is_empty() || beat.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(PersistError::corrupt(
                db_path(),
                "dialogue beat id is not an authored id",
            ));
        }
        state.dialogue_heard.insert((
            u32::try_from(npc)
                .map_err(|_| PersistError::integrity(db_path(), "npc content id is negative"))?,
            beat,
        ));
    }
    for row in tx
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
        PersistError::integrity(db_path(), "lease generation exceeds signed 64-bit storage")
    })
}

fn map_sql(err: postgres::Error) -> PersistError {
    crate::postgres::map_sql_pub(err)
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    #[test]
    fn lease_and_channel_durations_are_explicit() {
        assert_eq!(CHARACTER_LEASE_EXPIRY, Duration::from_secs(60));
        assert_eq!(CHARACTER_LEASE_RENEWAL, Duration::from_secs(10));
        assert!(CHARACTER_LEASE_RENEWAL < CHARACTER_LEASE_EXPIRY);
        assert_eq!(CHANNEL_GENERATION_EXPIRY, Duration::from_secs(60));
        assert_eq!(CHANNEL_GENERATION_RENEWAL, Duration::from_secs(10));
        assert!(CHANNEL_GENERATION_RENEWAL < CHANNEL_GENERATION_EXPIRY);
    }

    #[test]
    fn expiry_sql_uses_clock_timestamp_after_the_lock() {
        let source = include_str!("lifecycle.rs");
        assert!(source.contains("clock_timestamp()"));
        for line in source.lines() {
            let sql = line.contains("SELECT")
                || line.contains("UPDATE")
                || line.contains("INSERT")
                || line.contains("DELETE")
                || line.contains("WHERE");
            if sql {
                assert!(
                    !line.contains("transaction_timestamp"),
                    "expiry SQL must read the clock after a lock wait: {line}"
                );
            }
        }
    }
}
