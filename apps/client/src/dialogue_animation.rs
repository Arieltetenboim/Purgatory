//! Client-local catalogue for authored dialogue animation cues.
//!
//! NPC authoring stores logical `.anim` stems. This bridge loads those assets
//! into the existing Animation Runtime; it does not create dialogue gameplay
//! state or a second sampler/player implementation.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use purgatory_animation::{AnimationClip, parse_animation_asset_v1};
use purgatory_skeleton::humanoid_v0;

#[derive(Default)]
pub(crate) struct DialogueAnimationCatalog {
    clips: HashMap<String, AnimationClip>,
    issues: Vec<String>,
}

impl DialogueAnimationCatalog {
    #[must_use]
    pub(crate) fn load(content_root: &Path) -> Self {
        let root = content_root.join("shared").join("animations");
        let mut paths = Vec::new();
        let mut issues = Vec::new();
        collect_animation_paths(&root, &mut paths, &mut issues);
        paths.sort();

        let mut clips = HashMap::new();
        let mut first_path_by_id = HashMap::<String, PathBuf>::new();
        let mut ambiguous = HashSet::new();
        for path in paths {
            let Some(id) = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_owned)
            else {
                issues.push(format!(
                    "{}: animation filename is not UTF-8",
                    path.display()
                ));
                continue;
            };
            if let Some(previous) = first_path_by_id.get(&id) {
                clips.remove(&id);
                ambiguous.insert(id.clone());
                issues.push(format!(
                    "dialogue animation id '{id}' is ambiguous: {} and {}",
                    previous.display(),
                    path.display()
                ));
                continue;
            }
            first_path_by_id.insert(id.clone(), path.clone());

            let text = match fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) => {
                    issues.push(format!("{}: {error}", path.display()));
                    continue;
                }
            };
            match parse_animation_asset_v1(path.display().to_string(), &text, humanoid_v0()) {
                Ok(asset) => {
                    clips.insert(id, asset.clip);
                }
                Err(error) => issues.push(format!("{}: {error}", path.display())),
            }
        }
        for id in ambiguous {
            clips.remove(&id);
        }
        Self { clips, issues }
    }

    #[must_use]
    pub(crate) fn clip(&self, id: &str) -> Option<&AnimationClip> {
        self.clips.get(id)
    }

    #[must_use]
    pub(crate) fn issues(&self) -> &[String] {
        &self.issues
    }
}

fn collect_animation_paths(directory: &Path, paths: &mut Vec<PathBuf>, issues: &mut Vec<String>) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            issues.push(format!(
                "{}: dialogue animation catalogue is unavailable",
                directory.display()
            ));
            return;
        }
        Err(error) => {
            issues.push(format!("{}: {error}", directory.display()));
            return;
        }
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_animation_paths(&path, paths, issues);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "anim")
        {
            paths.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(path: &Path, body: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut file = fs::File::create(path).unwrap();
        file.write_all(body.as_bytes()).unwrap();
    }

    fn valid_animation() -> &'static str {
        "schema_version 1\nduration 1.0\nloop Loop\n\ntrack head\nrot 0.0 0.0 Linear\nrot 1.0 0.1 Linear\nendtrack\n\nmarkers\nendmarkers\n"
    }

    #[test]
    fn loads_nested_logical_ids_and_skips_invalid_assets() {
        let root = std::env::temp_dir().join(format!(
            "purgatory-dialogue-animation-catalog-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        write(
            &root.join("shared/animations/npc/talk.anim"),
            valid_animation(),
        );
        write(
            &root.join("shared/animations/npc/broken.anim"),
            "not an animation",
        );

        let catalog = DialogueAnimationCatalog::load(&root);
        assert!(catalog.clip("talk").is_some());
        assert!(catalog.clip("broken").is_none());
        assert_eq!(catalog.issues().len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_stems_are_ambiguous_and_fall_back() {
        let root = std::env::temp_dir().join(format!(
            "purgatory-dialogue-animation-duplicate-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        write(
            &root.join("shared/animations/a/talk.anim"),
            valid_animation(),
        );
        write(
            &root.join("shared/animations/b/talk.anim"),
            valid_animation(),
        );

        let catalog = DialogueAnimationCatalog::load(&root);
        assert!(catalog.clip("talk").is_none());
        assert!(catalog.issues()[0].contains("ambiguous"));
        let _ = fs::remove_dir_all(root);
    }
}
