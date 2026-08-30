use std::path::Path;

use purgatory_common::{CharacterId, DevLogin};

use crate::character::{PersistentCharacter, PersistentCharacterSnapshot};
use crate::error::PersistError;
use crate::identity::DevIdentityStore;
use crate::repository::FileCharacterRepository;

/// Single-threaded owner of identity allocation and character files.
pub struct PersistenceService {
    identity: DevIdentityStore,
    repo: FileCharacterRepository,
}

impl PersistenceService {
    pub fn open(dir: &Path) -> Result<Self, PersistError> {
        Ok(Self {
            identity: DevIdentityStore::open(dir)?,
            repo: FileCharacterRepository::open(dir)?,
        })
    }

    pub fn resolve_or_create(
        &mut self,
        login: &DevLogin,
    ) -> Result<PersistentCharacter, PersistError> {
        let id = self.identity.lookup_or_allocate(login)?;
        self.repo.load_or_default(id)
    }

    pub fn save_snapshot(
        &mut self,
        snapshot: PersistentCharacterSnapshot,
    ) -> Result<(), PersistError> {
        self.repo.save(&snapshot.into_character())
    }

    #[must_use]
    pub fn lookup(&self, login: &DevLogin) -> Option<CharacterId> {
        self.identity.lookup(login)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_dir() -> std::path::PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "purgatory-service-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn first_login_creates_character() {
        let dir = unique_dir();
        let mut svc = PersistenceService::open(&dir).unwrap();
        let login = DevLogin::parse("dev.local").unwrap();
        let a = svc.resolve_or_create(&login).unwrap();
        let b = svc.resolve_or_create(&login).unwrap();
        assert_eq!(a.character_id, b.character_id);
        assert_eq!(a.restore.map_authored, "map.dev.footnote");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
