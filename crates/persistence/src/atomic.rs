//! Crash-safe recoverable file replacement.
//!
//! On Windows, `rename` cannot replace an existing destination. The sequence
//! is therefore:
//!
//! ```text
//! write tmp → sync → dest → .bak (if dest exists) → tmp → dest → delete .bak
//! ```
//!
//! This is **crash-safe recoverable replacement**, not a stronger atomic
//! replace: there is a window where `dest` is absent and only `.bak` remains.
//! Recovery restores `.bak` to `dest`. `.tmp` is never authoritative.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub fn tmp_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_os_string();
    s.push(".tmp");
    PathBuf::from(s)
}

pub fn bak_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_os_string();
    s.push(".bak");
    PathBuf::from(s)
}

/// Recover `dest` from `.bak` when `dest` is missing. Stray `.tmp` is deleted.
pub fn recover_if_needed(dest: &Path) -> io::Result<()> {
    let tmp = tmp_path(dest);
    let bak = bak_path(dest);
    if dest.exists() {
        let _ = fs::remove_file(&tmp);
        return Ok(());
    }
    if bak.exists() {
        fs::rename(&bak, dest)?;
    }
    let _ = fs::remove_file(&tmp);
    Ok(())
}

/// Write `bytes` to `dest` via tmp/bak. Last valid committed state remains
/// recoverable. `.tmp` is never treated as committed.
pub fn replace_file_recoverable(dest: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    recover_if_needed(dest)?;
    let tmp = tmp_path(dest);
    let bak = bak_path(dest);
    {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    if dest.exists() {
        let _ = fs::remove_file(&bak);
        fs::rename(dest, &bak)?;
    }
    fs::rename(&tmp, dest)?;
    let _ = fs::remove_file(&bak);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_dir() -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "purgatory-atomic-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn tmp_is_never_authoritative() {
        let dir = unique_dir();
        let dest = dir.join("state.json");
        fs::write(&dest, b"{\"ok\":1}").unwrap();
        let tmp = tmp_path(&dest);
        fs::write(&tmp, b"garbage").unwrap();
        recover_if_needed(&dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"{\"ok\":1}");
        assert!(!tmp.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bak_recovers_when_dest_missing() {
        let dir = unique_dir();
        let dest = dir.join("state.json");
        let bak = bak_path(&dest);
        fs::write(&bak, b"committed").unwrap();
        recover_if_needed(&dest).unwrap();
        assert_eq!(fs::read_to_string(&dest).unwrap(), "committed");
        assert!(!bak.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn replace_roundtrip() {
        let dir = unique_dir();
        let dest = dir.join("state.json");
        replace_file_recoverable(&dest, b"one").unwrap();
        replace_file_recoverable(&dest, b"two").unwrap();
        assert_eq!(fs::read_to_string(&dest).unwrap(), "two");
        assert!(!tmp_path(&dest).exists());
        assert!(!bak_path(&dest).exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
