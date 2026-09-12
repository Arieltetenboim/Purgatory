//! Workspace-scoped Hub lock. Distinct from PowerShell `Local\PurgatoryDevLauncher`.

use std::fs::{File, OpenOptions};

use crate::paths::WorkspacePaths;

pub struct WorkspaceLock {
    _file: File,
}

impl WorkspaceLock {
    pub fn acquire(paths: &WorkspacePaths) -> Result<Self, String> {
        let dir = paths.dev_log_dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        let path = dir.join("hub.lock");
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|e| format!("open {}: {e}", path.display()))?;
        file.try_lock()
            .map_err(|_| "Another Developer Hub is already open for this workspace.".to_string())?;
        Ok(Self { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::WorkspacePaths;
    use std::fs;

    fn root(tag: &str) -> WorkspacePaths {
        let dir =
            std::env::temp_dir().join(format!("purgatory-hub-lock-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("PHASE"), "6G\n").unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            "[workspace.package]\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        WorkspacePaths::from_root(dir).unwrap()
    }

    #[test]
    fn second_lock_on_same_workspace_fails() {
        let paths = root("exclusive");
        let first = WorkspaceLock::acquire(&paths).expect("first lock");
        let second = WorkspaceLock::acquire(&paths);
        assert!(second.is_err(), "second Hub must be refused");
        drop(first);
        WorkspaceLock::acquire(&paths).expect("lock released after drop");
    }
}
