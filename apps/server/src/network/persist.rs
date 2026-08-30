//! Persistence worker. Owns identity allocation and character files.
//!
//! The simulation thread only `try_send`s owned [`PersistentCharacterSnapshot`]
//! values. JSON and filesystem work happen here.

use std::path::{Path, PathBuf};
use std::time::Duration;

use purgatory_common::DevLogin;
use purgatory_persistence::{
    PersistError, PersistenceService, PersistentCharacter, PersistentCharacterSnapshot,
};

enum PersistCmd {
    Resolve {
        login: DevLogin,
        reply: tokio::sync::oneshot::Sender<Result<PersistentCharacter, PersistError>>,
    },
    Save(PersistentCharacterSnapshot),
    Shutdown {
        reply: tokio::sync::oneshot::Sender<()>,
    },
}

/// Cloneable handle. Connection tasks await resolve; the sim thread `try_send`s saves.
#[derive(Clone)]
pub struct PersistenceHandle {
    tx: tokio::sync::mpsc::Sender<PersistCmd>,
}

impl PersistenceHandle {
    pub fn spawn(dir: &Path) -> Result<Self, String> {
        let mut service =
            PersistenceService::open(dir).map_err(|err| format!("persistence open: {err}"))?;
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    PersistCmd::Resolve { login, reply } => {
                        let result = service.resolve_or_create(&login);
                        let _ = reply.send(result);
                    }
                    PersistCmd::Save(snapshot) => {
                        if let Err(err) = service.save_snapshot(snapshot) {
                            eprintln!("PURGATORY persist save failed: {err}");
                        }
                    }
                    PersistCmd::Shutdown { reply } => {
                        while let Ok(extra) = rx.try_recv() {
                            if let PersistCmd::Save(snapshot) = extra
                                && let Err(err) = service.save_snapshot(snapshot)
                            {
                                eprintln!("PURGATORY persist drain save failed: {err}");
                            }
                        }
                        let _ = reply.send(());
                        break;
                    }
                }
            }
        });
        Ok(Self { tx })
    }

    pub async fn resolve(&self, login: DevLogin) -> Result<PersistentCharacter, PersistError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(PersistCmd::Resolve { login, reply })
            .await
            .map_err(|_| {
                PersistError::corrupt(PathBuf::from("<worker>"), "persistence worker closed")
            })?;
        rx.await.map_err(|_| {
            PersistError::corrupt(
                PathBuf::from("<worker>"),
                "persistence worker dropped reply",
            )
        })?
    }

    pub fn try_save(&self, snapshot: PersistentCharacterSnapshot) -> bool {
        self.tx.try_send(PersistCmd::Save(snapshot)).is_ok()
    }

    pub async fn shutdown(self, timeout: Duration) {
        let (reply, rx) = tokio::sync::oneshot::channel();
        if self.tx.send(PersistCmd::Shutdown { reply }).await.is_err() {
            return;
        }
        let _ = tokio::time::timeout(timeout, rx).await;
    }
}

#[must_use]
pub fn data_dir_from_env() -> PathBuf {
    resolve_data_dir(|key| std::env::var(key))
}

/// Resolve the runtime persistence root.
///
/// `PURGATORY_DATA_DIR` wins when set and non-empty. Otherwise the default is
/// a per-user application-data directory, never the source/install tree.
pub(crate) fn resolve_data_dir(
    mut getenv: impl FnMut(&str) -> Result<String, std::env::VarError>,
) -> PathBuf {
    if let Ok(dir) = getenv("PURGATORY_DATA_DIR") {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    default_data_dir(getenv)
}

fn default_data_dir(mut getenv: impl FnMut(&str) -> Result<String, std::env::VarError>) -> PathBuf {
    #[cfg(windows)]
    {
        getenv("LOCALAPPDATA")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Purgatory")
    }
    #[cfg(not(windows))]
    {
        if let Ok(xdg) = getenv("XDG_DATA_HOME") {
            let trimmed = xdg.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed).join("purgatory");
            }
        }
        getenv("HOME")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(|home| {
                PathBuf::from(home)
                    .join(".local")
                    .join("share")
                    .join("purgatory")
            })
            .unwrap_or_else(|| PathBuf::from("/var/tmp/purgatory"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::VarError;
    use std::path::PathBuf;

    fn env_map<'a>(
        pairs: &'a [(&'a str, &'a str)],
    ) -> impl FnMut(&str) -> Result<String, VarError> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_string())
                .ok_or(VarError::NotPresent)
        }
    }

    #[test]
    fn override_wins_over_platform_default() {
        let dir = resolve_data_dir(env_map(&[
            ("PURGATORY_DATA_DIR", r"D:\forced\persist"),
            ("LOCALAPPDATA", r"C:\should-not-use"),
            ("XDG_DATA_HOME", "/should-not-use"),
            ("HOME", "/should-not-use"),
        ]));
        assert_eq!(dir, PathBuf::from(r"D:\forced\persist"));
    }

    #[test]
    fn empty_override_falls_through() {
        let dir = resolve_data_dir(env_map(&[("PURGATORY_DATA_DIR", "  ")]));
        assert_ne!(dir, PathBuf::from("data"));
        assert_ne!(dir.as_os_str(), "data");
    }

    #[test]
    fn default_is_never_repo_relative_data() {
        let dir = resolve_data_dir(env_map(&[]));
        assert_ne!(dir, PathBuf::from("data"));
        assert_ne!(dir.as_os_str(), "data");
    }

    #[cfg(windows)]
    #[test]
    fn windows_default_is_localappdata_purgatory() {
        let dir = resolve_data_dir(env_map(&[("LOCALAPPDATA", r"C:\fake-local")]));
        assert_eq!(dir, PathBuf::from(r"C:\fake-local\Purgatory"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_missing_localappdata_is_still_not_repo_data() {
        let dir = resolve_data_dir(env_map(&[]));
        assert_eq!(dir, PathBuf::from(".").join("Purgatory"));
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_default_prefers_xdg_then_home() {
        let xdg = resolve_data_dir(env_map(&[("XDG_DATA_HOME", "/xdg/data")]));
        assert_eq!(xdg, PathBuf::from("/xdg/data/purgatory"));
        let home = resolve_data_dir(env_map(&[("HOME", "/home/dev")]));
        assert_eq!(home, PathBuf::from("/home/dev/.local/share/purgatory"));
    }

    #[tokio::test]
    async fn shutdown_drains_pending_saves_in_temp_dir() {
        let dir = std::env::temp_dir().join(format!(
            "purgatory-persist-drain-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let text = dir.to_string_lossy().to_ascii_lowercase();
        assert!(!text.contains("localappdata"));
        assert!(!text.contains("appdata\\purgatory"));

        let handle = PersistenceHandle::spawn(&dir).expect("spawn");
        let id = purgatory_common::CharacterId::from_raw(21);
        let character = purgatory_persistence::PersistentCharacter::new_default(id);
        let snapshot =
            purgatory_persistence::PersistentCharacterSnapshot::from_character(&character);
        assert!(handle.try_save(snapshot));
        handle.shutdown(Duration::from_secs(2)).await;
        let path = dir.join(purgatory_persistence::character_file_name(id));
        assert!(
            path.exists(),
            "shutdown must drain the pending save into the temp tree"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
