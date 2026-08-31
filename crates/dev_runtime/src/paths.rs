use std::path::{Path, PathBuf};

use crate::config::{LOAD_STEM, SERVER_STEM};

#[derive(Clone, Debug)]
pub struct WorkspacePaths {
    pub root: PathBuf,
}

impl WorkspacePaths {
    pub fn detect() -> Result<Self, String> {
        if let Ok(override_root) = std::env::var("PURGATORY_DEV_ROOT") {
            return Self::from_root(PathBuf::from(override_root));
        }
        if let Ok(cwd) = std::env::current_dir()
            && let Some(root) = walk_for_root(&cwd)
        {
            return Ok(Self { root });
        }
        if let Ok(exe) = std::env::current_exe()
            && let Some(start) = exe.parent()
            && let Some(root) = walk_for_root(start)
        {
            return Ok(Self { root });
        }
        Err("could not find PURGATORY workspace root (PHASE + Cargo.toml)".to_string())
    }

    pub fn from_root(root: PathBuf) -> Result<Self, String> {
        if !root.join("PHASE").is_file() || !root.join("Cargo.toml").is_file() {
            return Err(format!(
                "not a PURGATORY workspace root: {}",
                root.display()
            ));
        }
        Ok(Self { root })
    }

    pub fn target_prefix(&self) -> PathBuf {
        self.root.join("target")
    }

    pub fn profile_dir(&self) -> PathBuf {
        self.target_prefix().join("debug")
    }

    pub fn server_exe(&self) -> PathBuf {
        self.profile_dir().join(exe_name(SERVER_STEM))
    }

    pub fn load_exe(&self) -> PathBuf {
        self.profile_dir().join(exe_name(LOAD_STEM))
    }

    pub fn dev_log_dir(&self) -> PathBuf {
        self.root.join("logs").join("dev-tools")
    }
}

pub fn exe_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

fn walk_for_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join("PHASE").is_file() && dir.join("Cargo.toml").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        if cfg!(windows) {
            let exe = dir.join(format!("{name}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}
