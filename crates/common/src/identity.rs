//! Identity domains that must stay distinct from runtime [`EntityId`].
//!
//! Runtime instance identity lives in `purgatory-simulation` as generational
//! `EntityId`. This module only holds **content** and **persistent** boundaries.
//!
//! Stable numeric content IDs are the forward architecture contract. The current
//! authored-string/FNV constructor remains only as a temporary migration bridge
//! for content that has not yet moved to the numeric catalog.

/// Maximum length of a legacy authored content label.
pub const MAX_AUTHORED_CONTENT_ID_LEN: usize = 80;

pub const CONTENT_ID_BLOCK_SIZE: u32 = 10_000;
pub const CONTENT_MONSTER_START: u32 = 10_000;
pub const CONTENT_MONSTER_END: u32 = 19_999;
pub const CONTENT_NPC_START: u32 = 20_000;
pub const CONTENT_NPC_END: u32 = 29_999;
pub const CONTENT_ITEM_START: u32 = 30_000;
pub const CONTENT_ITEM_END: u32 = 39_999;
pub const CONTENT_ABILITY_START: u32 = 40_000;
pub const CONTENT_ABILITY_END: u32 = 49_999;
pub const CONTENT_MAP_START: u32 = 50_000;
pub const CONTENT_MAP_END: u32 = 59_999;
pub const CONTENT_WORLD_OBJECT_START: u32 = 60_000;
pub const CONTENT_WORLD_OBJECT_END: u32 = 69_999;

/// Broad durable content domain encoded by the stable numeric catalog block.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ContentKind {
    Monster,
    Npc,
    Item,
    Ability,
    Map,
    WorldObject,
}

/// Stable authored definition identity (template / content).
///
/// New durable content must use [`ContentId::from_raw`] with a catalog number in
/// the explicitly allocated domain block. [`ContentId::from_authored`] is a
/// temporary compatibility bridge for the pre-migration content pack and must
/// not be used to allocate new durable content IDs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ContentId {
    token: u64,
}

/// Why a legacy authored content label was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthoredIdError {
    Empty,
    TooLong,
    InvalidChar,
}

impl ContentId {
    /// Canonical stable numeric catalog constructor.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self { token: raw as u64 }
    }

    /// Numeric value when this ID fits the stable u32 catalog representation.
    #[must_use]
    pub const fn raw(self) -> Option<u32> {
        if self.token <= u32::MAX as u64 {
            Some(self.token as u32)
        } else {
            None
        }
    }

    /// Broad catalog domain for an allocated stable numeric ID.
    /// Reserved/unallocated values and legacy FNV tokens return `None`.
    #[must_use]
    pub const fn kind(self) -> Option<ContentKind> {
        let Some(raw) = self.raw() else {
            return None;
        };
        match raw {
            CONTENT_MONSTER_START..=CONTENT_MONSTER_END => Some(ContentKind::Monster),
            CONTENT_NPC_START..=CONTENT_NPC_END => Some(ContentKind::Npc),
            CONTENT_ITEM_START..=CONTENT_ITEM_END => Some(ContentKind::Item),
            CONTENT_ABILITY_START..=CONTENT_ABILITY_END => Some(ContentKind::Ability),
            CONTENT_MAP_START..=CONTENT_MAP_END => Some(ContentKind::Map),
            CONTENT_WORLD_OBJECT_START..=CONTENT_WORLD_OBJECT_END => Some(ContentKind::WorldObject),
            _ => None,
        }
    }

    /// Legacy migration bridge: authored label -> FNV token.
    ///
    /// This is not the forward content identity contract. New durable content
    /// must be assigned an explicit numeric catalog ID instead.
    pub fn from_authored(id: &str) -> Result<Self, AuthoredIdError> {
        validate_authored_id(id)?;
        Ok(Self {
            token: fnv1a64(id.as_bytes()),
        })
    }

    /// Wire/test compatibility constructor used by the current 8-byte protocol.
    /// Numeric catalog IDs round-trip through this unchanged; legacy tokens are
    /// tolerated until the protocol/content migration slice removes them.
    #[must_use]
    pub const fn from_token(token: u64) -> Self {
        Self { token }
    }

    /// Current protocol representation. Kept at u64 until the intentional wire migration.
    #[must_use]
    pub const fn token(self) -> u64 {
        self.token
    }
}

impl std::fmt::Display for ContentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.kind().is_some() {
            write!(f, "content#{}", self.token)
        } else {
            write!(f, "content#{:016x}", self.token)
        }
    }
}

/// Optional persistent-domain identity for a runtime entity.
///
/// Not a runtime `EntityId` (that type lives in simulation). Not
/// [`CharacterId`]. Phase 6E attaches `CharacterId` to the server ownership
/// model; this placeholder is **not** a Character handle and must not be
/// constructed from `CharacterId::raw()`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct PersistentId {
    token: u64,
}

impl PersistentId {
    #[must_use]
    pub const fn from_token(token: u64) -> Self {
        Self { token }
    }

    #[must_use]
    pub const fn token(self) -> u64 {
        self.token
    }
}

impl std::fmt::Display for PersistentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "persist#{}", self.token)
    }
}

/// Server-minted persistent character identity. Distinct from `EntityId`,
/// `ConnectionId`, `ContentId`, and [`PersistentId`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct CharacterId(u64);

impl CharacterId {
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for CharacterId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "char#{:016x}", self.0)
    }
}

/// Server-minted authoritative item-instance identity.
///
/// This identifies one economic item (or one stack), never its authored
/// [`ContentId`], an `EntityId`, a `CharacterId`, or client-provided input.
/// `from_raw` exists for authoritative storage/restore boundaries and tests;
/// server gameplay is responsible for minting values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ItemInstanceId(u64);

impl ItemInstanceId {
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for ItemInstanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "item#{:016x}", self.0)
    }
}

/// DEV-only lookup identity. Temporary until a real account system exists.
/// Exact validated input is the lookup key. Never used as a filesystem path.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct DevLogin(String);

/// Why a DEV login string was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DevLoginError {
    Empty,
    TooShort,
    TooLong,
    InvalidChar,
    DoubleDot,
    PathSeparator,
    LeadingOrTrailingDot,
}

pub const DEV_LOGIN_MIN_LEN: usize = 2;
pub const DEV_LOGIN_MAX_LEN: usize = 32;
pub const DEFAULT_DEV_LOGIN: &str = "dev.local";
pub const DEFAULT_RESTORE_POINT: &str = "default";

impl DevLogin {
    /// Parse without normalizing. Exact input is the lookup key.
    pub fn parse(raw: &str) -> Result<Self, DevLoginError> {
        if raw.is_empty() {
            return Err(DevLoginError::Empty);
        }
        if raw.len() < DEV_LOGIN_MIN_LEN {
            return Err(DevLoginError::TooShort);
        }
        if raw.len() > DEV_LOGIN_MAX_LEN {
            return Err(DevLoginError::TooLong);
        }
        if raw.contains("..") {
            return Err(DevLoginError::DoubleDot);
        }
        if raw.contains('/') || raw.contains('\\') {
            return Err(DevLoginError::PathSeparator);
        }
        if raw.starts_with('.') || raw.ends_with('.') {
            return Err(DevLoginError::LeadingOrTrailingDot);
        }
        if !raw
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')
        {
            return Err(DevLoginError::InvalidChar);
        }
        Ok(Self(raw.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DevLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Persistent restore intent. Not a [`crate::WorldAddress`]: Channel/Instance
/// are runtime placement, not restore semantics.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RestoreIntent {
    pub map_authored: String,
    pub point_id: String,
    #[serde(default)]
    pub checkpoint_id: Option<String>,
}

impl RestoreIntent {
    #[must_use]
    pub fn footnote_default() -> Self {
        Self {
            map_authored: MAP_FOOTNOTE_AUTHORED.to_string(),
            point_id: DEFAULT_RESTORE_POINT.to_string(),
            checkpoint_id: None,
        }
    }
}

/// Future instance-exit metadata. Not a runtime `InstanceId`.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InstanceExitContext {
    #[serde(default)]
    pub reason: Option<String>,
}

/// Legacy development map labels. Stable map identity moves to numeric IDs.
pub const MAP_FOOTNOTE_AUTHORED: &str = "map.dev.footnote";
pub const MAP_SECOND_AUTHORED: &str = "map.dev.second";

/// FNV-1a 64-bit used only by the temporary authored-string migration bridge.
#[must_use]
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    let mut hash = OFFSET;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// Legacy authored label validation: `segment.segment` with lowercase `[a-z0-9_]` segments.
pub fn validate_authored_id(id: &str) -> Result<(), AuthoredIdError> {
    if id.is_empty() {
        return Err(AuthoredIdError::Empty);
    }
    if id.len() > MAX_AUTHORED_CONTENT_ID_LEN {
        return Err(AuthoredIdError::TooLong);
    }
    let mut segments = 0u32;
    for segment in id.split('.') {
        if segment.is_empty() {
            return Err(AuthoredIdError::InvalidChar);
        }
        let mut chars = segment.chars();
        let Some(first) = chars.next() else {
            return Err(AuthoredIdError::InvalidChar);
        };
        if !first.is_ascii_lowercase() {
            return Err(AuthoredIdError::InvalidChar);
        }
        if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            return Err(AuthoredIdError::InvalidChar);
        }
        segments += 1;
    }
    if segments < 2 {
        return Err(AuthoredIdError::InvalidChar);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_and_persistent_ids_are_distinct_types() {
        let content = ContentId::from_token(1);
        let persistent = PersistentId::from_token(1);
        assert_eq!(content.token(), persistent.token());
        assert_eq!(content.to_string(), "content#0000000000000001");
        assert_eq!(persistent.to_string(), "persist#1");
    }

    #[test]
    fn stable_numeric_content_id_is_canonical_catalog_form() {
        let item = ContentId::from_raw(30_001);
        assert_eq!(item.raw(), Some(30_001));
        assert_eq!(item.kind(), Some(ContentKind::Item));
        assert_eq!(item.token(), 30_001);
        assert_eq!(item.to_string(), "content#30001");
    }

    #[test]
    fn content_domain_blocks_are_exact_and_reserved_space_is_unclassified() {
        assert_eq!(
            ContentId::from_raw(10_000).kind(),
            Some(ContentKind::Monster)
        );
        assert_eq!(
            ContentId::from_raw(19_999).kind(),
            Some(ContentKind::Monster)
        );
        assert_eq!(ContentId::from_raw(20_000).kind(), Some(ContentKind::Npc));
        assert_eq!(ContentId::from_raw(30_000).kind(), Some(ContentKind::Item));
        assert_eq!(
            ContentId::from_raw(40_000).kind(),
            Some(ContentKind::Ability)
        );
        assert_eq!(ContentId::from_raw(50_000).kind(), Some(ContentKind::Map));
        assert_eq!(
            ContentId::from_raw(60_000).kind(),
            Some(ContentKind::WorldObject)
        );
        assert_eq!(ContentId::from_raw(9_999).kind(), None);
        assert_eq!(ContentId::from_raw(70_000).kind(), None);
    }

    #[test]
    fn content_id_equality_is_by_numeric_token() {
        assert_eq!(ContentId::from_raw(30_007), ContentId::from_token(30_007));
        assert_ne!(ContentId::from_raw(30_007), ContentId::from_raw(30_008));
    }

    #[test]
    fn legacy_authored_constructor_remains_only_for_migration() {
        let a = ContentId::from_authored("map.dev.footnote").expect("legacy label");
        let b = ContentId::from_authored("map.dev.footnote").expect("legacy label");
        assert_eq!(a, b);
        assert_eq!(a.token(), fnv1a64(b"map.dev.footnote"));
        assert_eq!(a.kind(), None);
    }

    #[test]
    fn authored_label_rejects_malformed() {
        assert_eq!(validate_authored_id(""), Err(AuthoredIdError::Empty));
        assert_eq!(
            validate_authored_id("map"),
            Err(AuthoredIdError::InvalidChar)
        );
        assert_eq!(
            validate_authored_id("Map.dev"),
            Err(AuthoredIdError::InvalidChar)
        );
        assert_eq!(
            validate_authored_id(".dev.footnote"),
            Err(AuthoredIdError::InvalidChar)
        );
        assert!(validate_authored_id("entity.interactable.switch").is_ok());
    }

    #[test]
    fn character_id_is_not_persistent_id() {
        let character = CharacterId::from_raw(1);
        let persistent = PersistentId::from_token(1);
        assert_eq!(character.raw(), persistent.token());
        assert_eq!(character.to_string(), "char#0000000000000001");
        assert_eq!(persistent.to_string(), "persist#1");
    }

    #[test]
    fn item_instance_id_is_a_distinct_opaque_identity() {
        let item = ItemInstanceId::from_raw(1);
        let character = CharacterId::from_raw(1);
        let content = ContentId::from_token(1);
        assert_eq!(item.raw(), character.raw());
        assert_eq!(item.raw(), content.token());
        assert_eq!(item.to_string(), "item#0000000000000001");
    }

    #[test]
    fn dev_login_accepts_reasonable_examples() {
        for raw in ["dev.local", "dev.local.b", "alice", "client1", "bot.0001"] {
            assert_eq!(DevLogin::parse(raw).unwrap().as_str(), raw);
        }
    }

    #[test]
    fn dev_login_rejects_malformed() {
        assert_eq!(DevLogin::parse(""), Err(DevLoginError::Empty));
        assert_eq!(DevLogin::parse("a"), Err(DevLoginError::TooShort));
        assert_eq!(
            DevLogin::parse(&"a".repeat(DEV_LOGIN_MAX_LEN + 1)),
            Err(DevLoginError::TooLong)
        );
        assert_eq!(DevLogin::parse("Alice"), Err(DevLoginError::InvalidChar));
        assert_eq!(DevLogin::parse("dev..local"), Err(DevLoginError::DoubleDot));
        assert_eq!(
            DevLogin::parse("dev/local"),
            Err(DevLoginError::PathSeparator)
        );
        assert_eq!(
            DevLogin::parse("dev\\local"),
            Err(DevLoginError::PathSeparator)
        );
        assert_eq!(
            DevLogin::parse(".hidden"),
            Err(DevLoginError::LeadingOrTrailingDot)
        );
        assert_eq!(
            DevLogin::parse("trail."),
            Err(DevLoginError::LeadingOrTrailingDot)
        );
        assert!(DevLogin::parse(DEFAULT_DEV_LOGIN).is_ok());
    }

    #[test]
    fn restore_intent_is_not_a_world_address() {
        let intent = RestoreIntent::footnote_default();
        assert_eq!(intent.map_authored, MAP_FOOTNOTE_AUTHORED);
        assert_eq!(intent.point_id, DEFAULT_RESTORE_POINT);
        assert!(intent.checkpoint_id.is_none());
    }
}
