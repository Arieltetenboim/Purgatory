//! Workspace paths and .anim file IO.

use std::fs;
use std::path::{Path, PathBuf};

use purgatory_animation::{
    ValidatedAnimationAsset, parse_animation_asset_v1, serialize_animation_asset_v1,
};
use purgatory_skeleton::humanoid_v0;

use crate::document::AnimDocument;

pub fn find_workspace_root() -> Result<PathBuf, String> {
    if let Ok(override_root) = std::env::var("PURGATORY_DEV_ROOT") {
        return require_root(PathBuf::from(override_root));
    }
    if let Ok(cwd) = std::env::current_dir()
        && let Some(root) = walk_for_root(&cwd)
    {
        return Ok(root);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(start) = exe.parent()
        && let Some(root) = walk_for_root(start)
    {
        return Ok(root);
    }
    Err("could not find PURGATORY workspace root (PHASE + Cargo.toml)".to_string())
}

fn require_root(root: PathBuf) -> Result<PathBuf, String> {
    if root.join("PHASE").is_file() && root.join("Cargo.toml").is_file() {
        Ok(root)
    } else {
        Err(format!(
            "not a PURGATORY workspace root: {}",
            root.display()
        ))
    }
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

#[must_use]
pub fn animation_dev_dir(root: &Path) -> PathBuf {
    root.join("content")
        .join("shared")
        .join("animations")
        .join("dev")
}

#[must_use]
pub fn list_anim_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("anim") {
            files.push(path);
        }
    }
    files.sort();
    files
}

pub fn load_anim_file(path: &Path) -> Result<AnimDocument, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("clip.anim");
    let asset = parse_animation_asset_v1(name, &text, humanoid_v0()).map_err(|e| e.to_string())?;
    Ok(AnimDocument::from_asset(&asset))
}

pub fn save_anim_file(path: &Path, document: &AnimDocument) -> Result<(), String> {
    let asset = document.to_asset(humanoid_v0())?;
    write_validated(path, &asset)
}

pub fn write_validated(path: &Path, asset: &ValidatedAnimationAsset) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let text = serialize_animation_asset_v1(asset);
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("clip.anim");
    parse_animation_asset_v1(name, &text, humanoid_v0())
        .map_err(|e| format!("refusing to save; serialized text failed A6 parse: {e}"))?;
    fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

pub fn sanitize_clip_stem(name: &str) -> Result<String, String> {
    let stem = name.trim();
    let stem = stem.strip_suffix(".anim").unwrap_or(stem);
    if stem.is_empty() {
        return Err("clip name is empty".to_string());
    }
    if !stem
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err("clip name must be ascii alphanumeric, '_', '-', or '.'".to_string());
    }
    Ok(stem.to_string())
}

#[must_use]
pub fn clip_path_in_dev(dir: &Path, stem: &str) -> PathBuf {
    dir.join(format!("{stem}.anim"))
}
