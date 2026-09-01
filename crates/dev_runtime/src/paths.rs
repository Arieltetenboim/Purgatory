use std::path::{Path, PathBuf};

use crate::config::{CLIENT_STEM, LOAD_STEM, SERVER_STEM};
use crate::settings::BuildProfile;

#[derive(Clone, Debug)]
pub struct WorkspacePaths {
    pub root: PathBuf,
    pub profile: BuildProfile,
}

impl WorkspacePaths {
    pub fn detect() -> Result<Self, String> {
        if let Ok(override_root) = std::env::var("PURGATORY_DEV_ROOT") {
            return Self::from_root(PathBuf::from(override_root));
        }
        if let Ok(cwd) = std::env::current_dir()
            && let Some(root) = walk_for_root(&cwd)
        {
            return Ok(Self {
                root,
                profile: BuildProfile::Debug,
            });
        }
        if let Ok(exe) = std::env::current_exe()
            && let Some(start) = exe.parent()
            && let Some(root) = walk_for_root(start)
        {
            return Ok(Self {
                root,
                profile: BuildProfile::Debug,
            });
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
        Ok(Self {
            root,
            profile: BuildProfile::Debug,
        })
    }

    pub fn set_profile(&mut self, profile: BuildProfile) {
        self.profile = profile;
    }

    pub fn target_prefix(&self) -> PathBuf {
        self.root.join("target")
    }

    pub fn profile_dir(&self) -> PathBuf {
        self.target_prefix().join(self.profile.as_str())
    }

    pub fn server_exe(&self) -> PathBuf {
        self.profile_dir().join(exe_name(SERVER_STEM))
    }

    pub fn client_exe(&self) -> PathBuf {
        self.profile_dir().join(exe_name(CLIENT_STEM))
    }

    pub fn load_exe(&self) -> PathBuf {
        self.profile_dir().join(exe_name(LOAD_STEM))
    }

    pub fn dev_log_dir(&self) -> PathBuf {
        self.root.join("logs").join("dev-tools")
    }

    pub fn check_script(&self) -> PathBuf {
        self.root.join("scripts").join("check.ps1")
    }
}

pub fn exe_name(stem: &str) -> String {
    #[cfg(windows)]
    {
        format!("{stem}.exe")
    }
    #[cfg(not(windows))]
    {
        stem.to_string()
    }
}

pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(exe_name(name));
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let bat = dir.join(format!("{name}.bat"));
            if bat.is_file() {
                return Some(bat);
            }
            let cmd = dir.join(format!("{name}.cmd"));
            if cmd.is_file() {
                return Some(cmd);
            }
        }
    }
    None
}

fn walk_for_root(start: &Path) -> Option<PathBuf> {
    let mut cur = start.to_path_buf();
    loop {
        if cur.join("PHASE").is_file() && cur.join("Cargo.toml").is_file() {
            return Some(cur);
        }
        if !cur.pop() {
            return None;
        }
    }
}
