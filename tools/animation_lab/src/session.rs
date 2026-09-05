//! Lab session: committed document, edit transactions, playback, IO.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use purgatory_animation::{Interpolation, Keyframe, LoopPolicy};
use purgatory_skeleton::{BoneIndex, humanoid_v0};

use crate::clipboard::{Clipboard, copy_keys, copy_pose, paste_keys};
use crate::document::{
    AnimDocument, ChannelKind, KeyRef, bone_label, clamp_key_time, sampled_channel_value,
    times_equal,
};
use crate::history::EditHistory;
use crate::io::{
    animation_dev_dir, clip_path_in_dev, find_workspace_root, list_anim_files, load_anim_file,
    sanitize_clip_stem, save_anim_file,
};
use crate::preview::{EvaluatedPreview, apply_direct_rotation, evaluate_preview_at};
use crate::snap::{JointSnapSettings, SnapSettings, snap_joint_rotation, snap_time};

#[derive(Clone, Debug)]
pub struct EditTransaction {
    pub label: String,
    pub before: AnimDocument,
    pub working: AnimDocument,
}

#[derive(Clone, Debug)]
pub struct NewClipForm {
    pub name: String,
    pub duration: String,
    pub loop_once: bool,
}

impl Default for NewClipForm {
    fn default() -> Self {
        Self {
            name: "untitled".to_string(),
            duration: "1.0".to_string(),
            loop_once: false,
        }
    }
}

pub struct LabSession {
    pub root: PathBuf,
    pub anim_dir: PathBuf,
    pub path: Option<PathBuf>,
    document: AnimDocument,
    pub history: EditHistory,
    pub playhead: f32,
    pub playing: bool,
    pub loop_preview: bool,
    pub selected_bone: Option<BoneIndex>,
    pub selected_channel: ChannelKind,
    pub selected_key: Option<usize>,
    pub selected_marker: Option<usize>,
    pub status: String,
    pub last_error: Option<String>,
    pub saved_document: Option<AnimDocument>,
    pub transaction: Option<EditTransaction>,
    pub new_clip: NewClipForm,
    pub save_as_name: String,
    pub duration_draft: String,
    pub key_time_draft: String,
    pub key_value_draft: String,
    pub marker_name_draft: String,
    pub marker_type_draft: String,
    pub marker_time_draft: String,
    pub selected_keys: HashSet<KeyRef>,
    pub selection_anchor: Option<KeyRef>,
    pub clipboard: Clipboard,
    pub snap: SnapSettings,
    pub joint_snap: JointSnapSettings,
    pub mirror_left: bool,
    pub transition_enabled: bool,
    pub transition_b: Option<(PathBuf, AnimDocument)>,
    pub transition_duration: f32,
    pub transition_alpha: f32,
    pub transition_playing: bool,
}

impl LabSession {
    pub fn open_workspace() -> Result<Self, String> {
        let root = find_workspace_root()?;
        let anim_dir = animation_dev_dir(&root);
        let document = AnimDocument::empty(1.0, LoopPolicy::Loop)?;
        Ok(Self::from_document(
            root, anim_dir, None, document, "New clip", false,
        ))
    }

    #[cfg(test)]
    pub fn for_test(root: PathBuf, anim_dir: PathBuf, document: AnimDocument) -> Self {
        Self::from_document(root, anim_dir, None, document, "test", false)
    }

    fn from_document(
        root: PathBuf,
        anim_dir: PathBuf,
        path: Option<PathBuf>,
        document: AnimDocument,
        history_label: impl Into<String>,
        mark_saved: bool,
    ) -> Self {
        let duration_draft = format!("{:.2}", document.duration);
        let save_as_name = path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("untitled")
            .to_string();
        let saved = if mark_saved {
            Some(document.clone())
        } else {
            None
        };
        Self {
            root,
            anim_dir,
            path,
            history: EditHistory::new(history_label, document.clone()),
            document,
            playhead: 0.0,
            playing: false,
            loop_preview: true,
            selected_bone: None,
            selected_channel: ChannelKind::Rotation,
            selected_key: None,
            selected_marker: None,
            status: String::new(),
            last_error: None,
            saved_document: saved,
            transaction: None,
            new_clip: NewClipForm::default(),
            save_as_name,
            duration_draft,
            key_time_draft: String::new(),
            key_value_draft: String::new(),
            marker_name_draft: "marker".to_string(),
            marker_type_draft: "notify".to_string(),
            marker_time_draft: "0.00".to_string(),
            selected_keys: HashSet::new(),
            selection_anchor: None,
            clipboard: Clipboard::default(),
            snap: SnapSettings::default(),
            joint_snap: JointSnapSettings::default(),
            mirror_left: false,
            transition_enabled: false,
            transition_b: None,
            transition_duration: 0.20,
            transition_alpha: 0.0,
            transition_playing: false,
        }
    }

    #[must_use]
    pub fn document(&self) -> &AnimDocument {
        self.transaction
            .as_ref()
            .map(|t| &t.working)
            .unwrap_or(&self.document)
    }

    #[must_use]
    pub fn committed_document(&self) -> &AnimDocument {
        &self.document
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        if self.transaction.is_some() {
            return true;
        }
        match &self.saved_document {
            None => true,
            Some(saved) => saved != &self.document,
        }
    }

    #[must_use]
    pub fn asset_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "(unsaved)".to_string())
    }

    pub fn list_files(&self) -> Vec<PathBuf> {
        list_anim_files(&self.anim_dir)
    }

    pub fn open_path(&mut self, path: &Path) -> Result<(), String> {
        if self.transaction.is_some() {
            return Err("finish or cancel the current drag first".to_string());
        }
        let document = load_anim_file(path)?;
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("clip.anim");
        *self = Self::from_document(
            self.root.clone(),
            self.anim_dir.clone(),
            Some(path.to_path_buf()),
            document,
            format!("Open {name}"),
            true,
        );
        self.status = format!("Opened {name}");
        self.last_error = None;
        Ok(())
    }

    pub fn new_clip(&mut self) -> Result<(), String> {
        if self.transaction.is_some() {
            return Err("finish or cancel the current drag first".to_string());
        }
        let stem = sanitize_clip_stem(&self.new_clip.name)?;
        let duration: f32 = self
            .new_clip
            .duration
            .trim()
            .parse()
            .map_err(|_| "invalid duration".to_string())?;
        let loop_policy = if self.new_clip.loop_once {
            LoopPolicy::Once
        } else {
            LoopPolicy::Loop
        };
        let path = clip_path_in_dev(&self.anim_dir, &stem);
        if path.is_file() {
            return Err(format!(
                "{} already exists; Open it or pick another name",
                path.display()
            ));
        }
        let document = AnimDocument::empty(duration, loop_policy)?;
        *self = Self::from_document(
            self.root.clone(),
            self.anim_dir.clone(),
            Some(path),
            document,
            format!("New clip — {stem}.anim"),
            false,
        );
        self.status = format!("New clip {stem}.anim (unsaved)");
        self.last_error = None;
        Ok(())
    }

    pub fn save(&mut self) -> Result<(), String> {
        let Some(path) = self.path.clone() else {
            return self.save_as();
        };
        self.save_to(&path)
    }

    pub fn save_as(&mut self) -> Result<(), String> {
        let stem = sanitize_clip_stem(&self.save_as_name)?;
        let path = clip_path_in_dev(&self.anim_dir, &stem);
        self.save_to(&path)?;
        self.path = Some(path);
        self.save_as_name = stem;
        Ok(())
    }

    fn save_to(&mut self, path: &Path) -> Result<(), String> {
        if self.transaction.is_some() {
            return Err("finish or cancel the current drag before save".to_string());
        }
        save_anim_file(path, &self.document)?;
        self.saved_document = Some(self.document.clone());
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("clip.anim");
        self.status = format!("Saved {name}");
        self.last_error = None;
        Ok(())
    }

    pub fn set_error(&mut self, err: impl Into<String>) {
        let err = err.into();
        self.last_error = Some(err.clone());
        self.status = err;
    }

    pub fn commit(&mut self, label: impl Into<String>, next: AnimDocument) -> Result<(), String> {
        next.validate()?;
        self.history.push(label, next.clone());
        self.document = next;
        self.sync_drafts_from_document();
        Ok(())
    }

    pub fn mutate<F>(&mut self, label: impl Into<String>, f: F) -> Result<(), String>
    where
        F: FnOnce(&mut AnimDocument) -> Result<(), String>,
    {
        if self.transaction.is_some() {
            return Err("finish or cancel the current drag first".to_string());
        }
        let mut next = self.document.clone();
        f(&mut next)?;
        next.validate()?;
        if next == self.document {
            return Ok(());
        }
        self.commit(label, next)
    }

    pub fn undo(&mut self) {
        if self.transaction.is_some() {
            self.cancel_transaction();
            return;
        }
        if let Some(doc) = self.history.undo() {
            self.document = doc.clone();
            self.sync_drafts_from_document();
            self.status = "Undo".to_string();
            self.last_error = None;
        }
    }

    pub fn redo(&mut self) {
        if self.transaction.is_some() {
            return;
        }
        if let Some(doc) = self.history.redo() {
            self.document = doc.clone();
            self.sync_drafts_from_document();
            self.status = "Redo".to_string();
            self.last_error = None;
        }
    }

    pub fn jump_history(&mut self, index: usize) {
        if self.transaction.is_some() {
            self.cancel_transaction();
        }
        if let Some(doc) = self.history.jump(index) {
            self.document = doc.clone();
            self.sync_drafts_from_document();
            self.status = format!("History → {}", self.history.entries()[index].label);
            self.last_error = None;
        }
    }

    pub fn begin_transaction(&mut self, label: impl Into<String>) {
        if self.transaction.is_some() {
            return;
        }
        self.transaction = Some(EditTransaction {
            label: label.into(),
            before: self.document.clone(),
            working: self.document.clone(),
        });
    }

    pub fn transaction_working_mut(&mut self) -> Option<&mut AnimDocument> {
        self.transaction.as_mut().map(|t| &mut t.working)
    }

    pub fn commit_transaction(&mut self) -> Result<(), String> {
        let Some(tx) = self.transaction.take() else {
            return Ok(());
        };
        if tx.working == tx.before {
            return Ok(());
        }
        if let Err(e) = tx.working.validate() {
            self.transaction = Some(tx);
            return Err(e);
        }
        self.commit(tx.label, tx.working)
    }

    pub fn cancel_transaction(&mut self) {
        self.transaction = None;
        self.status = "Drag cancelled".to_string();
    }

    pub fn apply_direct_rotation_working(
        &mut self,
        bone: BoneIndex,
        local_rotation: f32,
    ) -> Result<(), String> {
        let t = self.playhead;
        let snapped = snap_joint_rotation(local_rotation, self.joint_snap);
        let working = self
            .transaction_working_mut()
            .ok_or_else(|| "no active edit transaction".to_string())?;
        apply_direct_rotation(working, bone, t, snapped)
    }

    /// Live-preview a key-time move inside the open edit transaction.
    /// Returns the post-sort index of the moved key.
    pub fn preview_key_time(
        &mut self,
        bone: BoneIndex,
        kind: ChannelKind,
        index: usize,
        new_time: f32,
    ) -> Result<usize, String> {
        let working = self
            .transaction_working_mut()
            .ok_or_else(|| "no active edit transaction".to_string())?;
        working.move_key(bone, kind, index, new_time)?;
        let snapped = clamp_key_time(new_time, working.duration);
        let new_index = working
            .track(bone)
            .and_then(|tr| {
                tr.channel(kind)
                    .iter()
                    .position(|k| times_equal(k.time, snapped))
            })
            .unwrap_or(index);
        self.selected_bone = Some(bone);
        self.selected_channel = kind;
        self.selected_key = Some(new_index);
        self.sync_key_drafts();
        Ok(new_index)
    }

    pub fn add_key_at_playhead(&mut self) -> Result<(), String> {
        let bone = self
            .selected_bone
            .ok_or_else(|| "select a bone first".to_string())?;
        let kind = self.selected_channel;
        let t = self.snap_authoring_time(self.playhead, &[], None);
        let value = self.default_key_value(bone, kind, t);
        let label = format!("Add key — {}.{} @ {t:.2}", bone_label(bone), kind.token());
        self.mutate(label, |doc| {
            doc.upsert_key(
                bone,
                kind,
                Keyframe {
                    time: t,
                    value,
                    interpolation: Interpolation::Linear,
                },
            )
        })
    }

    fn default_key_value(&self, bone: BoneIndex, kind: ChannelKind, t: f32) -> f32 {
        if let Some(v) = sampled_channel_value(self.document(), bone, kind, t) {
            return v;
        }
        let def = humanoid_v0();
        let bind = def
            .bind_local(bone)
            .unwrap_or(purgatory_skeleton::BoneTransform::IDENTITY);
        match kind {
            ChannelKind::Rotation => bind.rotation,
            ChannelKind::TranslationX => bind.translation[0],
            ChannelKind::TranslationY => bind.translation[1],
            ChannelKind::DepthAngle => 0.0,
        }
    }

    pub fn delete_selected_key(&mut self) -> Result<(), String> {
        let refs = self.selection_refs();
        if refs.is_empty() {
            return Err("select a key first".to_string());
        }
        let n = refs.len();
        self.mutate(format!("Delete {n} key(s)"), |doc| doc.delete_keys(&refs))?;
        self.selected_keys.clear();
        self.selected_key = None;
        self.selection_anchor = None;
        Ok(())
    }

    pub fn commit_key_drafts(&mut self) -> Result<(), String> {
        let bone = self
            .selected_bone
            .ok_or_else(|| "select a bone first".to_string())?;
        let kind = self.selected_channel;
        let index = self
            .selected_key
            .ok_or_else(|| "select a key first".to_string())?;
        let time: f32 = self
            .key_time_draft
            .trim()
            .parse()
            .map_err(|_| "invalid key time".to_string())?;
        let value: f32 = self
            .key_value_draft
            .trim()
            .parse()
            .map_err(|_| "invalid key value".to_string())?;
        let old_time = self
            .document()
            .track(bone)
            .and_then(|tr| tr.channel(kind).get(index))
            .map(|k| k.time)
            .unwrap_or(time);
        let label = if (old_time - time).abs() > f32::EPSILON {
            format!(
                "Move key — {}.{} @ {old_time:.2} → {time:.2}",
                bone_label(bone),
                kind.token()
            )
        } else {
            format!(
                "Set key — {}.{} @ {old_time:.2}",
                bone_label(bone),
                kind.token()
            )
        };
        self.mutate(label, |doc| {
            let interp = doc
                .track(bone)
                .and_then(|tr| tr.channel(kind).get(index))
                .map(|k| k.interpolation)
                .unwrap_or(Interpolation::Linear);
            doc.move_key(bone, kind, index, time)?;
            let snapped = clamp_key_time(time, doc.duration);
            let mut key = doc
                .track(bone)
                .and_then(|tr| {
                    tr.channel(kind)
                        .iter()
                        .find(|k| times_equal(k.time, snapped))
                })
                .copied()
                .ok_or_else(|| "moved key not found".to_string())?;
            key.value = value;
            key.interpolation = interp;
            doc.upsert_key(bone, kind, key)?;
            Ok(())
        })?;
        if let Some(tr) = self.document.track(bone) {
            self.selected_key = tr
                .channel(kind)
                .iter()
                .position(|k| times_equal(k.time, clamp_key_time(time, self.document.duration)));
        }
        self.sync_key_drafts();
        Ok(())
    }

    pub fn commit_duration_draft(&mut self) -> Result<(), String> {
        let duration: f32 = self
            .duration_draft
            .trim()
            .parse()
            .map_err(|_| "invalid duration".to_string())?;
        let old = self.document.duration;
        if (old - duration).abs() <= f32::EPSILON {
            return Ok(());
        }
        self.mutate(
            format!("Change duration — {old:.2} → {duration:.2}"),
            |doc| doc.set_duration(duration),
        )
    }

    pub fn set_loop_policy(&mut self, loop_policy: LoopPolicy) -> Result<(), String> {
        let old = self.document.loop_policy;
        if old == loop_policy {
            return Ok(());
        }
        let label = format!("Change loop — {:?} → {:?}", old, loop_policy);
        self.mutate(label, |doc| doc.set_loop_policy(loop_policy))
    }

    pub fn add_marker_at_playhead(&mut self) -> Result<(), String> {
        let t = clamp_key_time(self.playhead, self.document.duration);
        let name = self.marker_name_draft.trim();
        if name.is_empty() {
            return Err("marker name is empty".to_string());
        }
        if name.contains(char::is_whitespace) {
            return Err("marker name must be a single token".to_string());
        }
        let marker_type = self.marker_type_draft.trim();
        if marker_type.is_empty() || marker_type.contains(char::is_whitespace) {
            return Err("marker type must be a single token".to_string());
        }
        let marker = purgatory_animation::AnimationMarker {
            time: t,
            name: name.to_string(),
            marker_type: marker_type.to_string(),
            payload: None,
        };
        let label = format!("Add marker — {} @ {t:.2}", marker.name);
        self.mutate(label, |doc| doc.upsert_marker(marker))
    }

    pub fn delete_selected_marker(&mut self) -> Result<(), String> {
        let index = self
            .selected_marker
            .ok_or_else(|| "select a marker first".to_string())?;
        let name = self
            .document
            .markers
            .get(index)
            .map(|m| m.name.clone())
            .unwrap_or_default();
        self.mutate(format!("Delete marker — {name}"), |doc| {
            doc.delete_marker(index).map(|_| ())
        })?;
        self.selected_marker = None;
        Ok(())
    }

    pub fn advance_playhead(&mut self, dt: f32) {
        if !self.playing || !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let duration = self.document().duration.max(KEY_TIME_FALLBACK);
        self.playhead += dt;
        if self.loop_preview {
            self.playhead = self.playhead.rem_euclid(duration);
        } else if self.playhead >= duration {
            self.playhead = duration;
            self.playing = false;
        }
    }

    pub fn set_playhead(&mut self, t: f32) {
        let duration = self.document().duration.max(0.0);
        self.playhead = t.clamp(0.0, duration);
    }

    pub fn preview_pose(&self) -> Result<EvaluatedPreview, String> {
        if !self.transition_enabled {
            return evaluate_preview_at(self.document(), self.playhead, None, self.playhead, 0.0);
        }
        let Some((_, other)) = &self.transition_b else {
            return Err("select a valid clip B for transition preview".to_string());
        };
        let t_b = (other.duration * self.transition_alpha).clamp(0.0, other.duration);
        evaluate_preview_at(
            self.document(),
            self.playhead,
            Some(other),
            t_b,
            self.transition_alpha,
        )
    }

    pub fn select_bone(&mut self, bone: BoneIndex) {
        self.selected_bone = Some(bone);
        self.selected_key = None;
        self.selected_keys.clear();
        self.selection_anchor = None;
        self.sync_key_drafts();
    }

    pub fn set_selected_channel(&mut self, kind: ChannelKind) {
        self.selected_channel = kind;
        self.selected_key = None;
        self.selected_keys.clear();
        self.selection_anchor = None;
        self.sync_key_drafts();
    }

    pub fn select_key(&mut self, index: usize) {
        self.selected_key = Some(index);
        if let (Some(bone), Some(i)) = (self.selected_bone, self.selected_key)
            && let Some(key) = self
                .document()
                .track(bone)
                .and_then(|tr| tr.channel(self.selected_channel).get(i))
        {
            let key_ref = KeyRef::from_key(bone, self.selected_channel, key.time);
            self.selected_keys.clear();
            self.selected_keys.insert(key_ref);
            self.selection_anchor = Some(key_ref);
        }
        self.sync_key_drafts();
    }

    pub fn selection_refs(&self) -> Vec<KeyRef> {
        let mut refs: Vec<_> = self.selected_keys.iter().copied().collect();
        if refs.is_empty()
            && let (Some(bone), Some(index)) = (self.selected_bone, self.selected_key)
            && let Some(key) = self
                .document()
                .track(bone)
                .and_then(|tr| tr.channel(self.selected_channel).get(index))
        {
            refs.push(KeyRef::from_key(bone, self.selected_channel, key.time));
        }
        refs
    }

    pub fn replace_selection(&mut self, refs: Vec<KeyRef>, primary: Option<KeyRef>) {
        self.selected_keys = refs.into_iter().collect();
        if let Some(p) = primary.or(self.selected_keys.iter().copied().next()) {
            self.selected_bone = Some(p.bone);
            self.selected_channel = p.kind;
            self.selected_key = self.document().track(p.bone).and_then(|tr| {
                tr.channel(p.kind)
                    .iter()
                    .position(|k| times_equal(k.time, p.time()))
            });
            self.selection_anchor = Some(p);
        }
        self.sync_key_drafts();
    }

    pub fn toggle_selection(&mut self, key_ref: KeyRef) {
        if !self.selected_keys.remove(&key_ref) {
            self.selected_keys.insert(key_ref);
        }
        self.selected_bone = Some(key_ref.bone);
        self.selected_channel = key_ref.kind;
        self.selected_key = self.document().track(key_ref.bone).and_then(|tr| {
            tr.channel(key_ref.kind)
                .iter()
                .position(|k| times_equal(k.time, key_ref.time()))
        });
        self.selection_anchor = Some(key_ref);
        self.sync_key_drafts();
    }

    pub fn copy_selection(&mut self) -> Result<(), String> {
        let refs = self.selection_refs();
        if refs.is_empty() {
            return Err("select a key first".to_string());
        }
        self.clipboard = copy_keys(self.document(), &refs);
        if self.clipboard.is_empty() {
            return Err("copied selection did not match any keys".to_string());
        }
        self.status = format!("Copied {} key(s)", self.clipboard.keys.len());
        self.last_error = None;
        Ok(())
    }

    pub fn copy_current_pose(&mut self) {
        self.clipboard = copy_pose(self.document(), self.playhead);
        self.status = format!("Copied pose ({} channels)", self.clipboard.keys.len());
        self.last_error = None;
    }

    pub fn paste_clipboard(&mut self) -> Result<(), String> {
        self.apply_clipboard_paste(false)
    }

    pub fn paste_pose(&mut self) -> Result<(), String> {
        self.apply_clipboard_paste(true)
    }

    fn apply_clipboard_paste(&mut self, pose: bool) -> Result<(), String> {
        let keys = paste_keys(self.document(), &self.clipboard, self.playhead)?;
        let n = keys.len();
        let label = if pose {
            "Paste pose".to_string()
        } else {
            format!("Paste {n} key(s)")
        };
        self.mutate(label, |doc| doc.upsert_keys(&keys))?;
        let pasted_refs: Vec<KeyRef> = keys
            .iter()
            .map(|(bone, kind, key)| KeyRef::from_key(*bone, *kind, key.time))
            .collect();
        self.replace_selection(pasted_refs, None);
        Ok(())
    }

    pub fn set_current_pose_as_clip_start(&mut self) -> Result<(), String> {
        let pose = copy_pose(self.document(), self.playhead);
        let keys = paste_keys(self.document(), &pose, 0.0)?;
        self.mutate("Set current pose as clip start", |doc| {
            doc.upsert_keys(&keys)
        })?;
        Ok(())
    }

    pub fn select_keys_at_playhead(&mut self) {
        let refs = self.document().keys_at_time(self.playhead);
        if refs.is_empty() {
            self.status = "No keys at playhead".to_string();
            return;
        }
        self.replace_selection(refs, None);
        self.status = format!("Selected {} key(s) at playhead", self.selected_keys.len());
        self.last_error = None;
    }

    pub fn delete_keys_at_playhead(&mut self) -> Result<(), String> {
        let refs = self.document().keys_at_time(self.playhead);
        if refs.is_empty() {
            return Err("no keys at playhead".to_string());
        }
        let n = refs.len();
        self.mutate(format!("Delete {n} key(s) at playhead"), |doc| {
            doc.delete_keys(&refs)
        })?;
        self.selected_keys.clear();
        self.selected_key = None;
        self.selection_anchor = None;
        Ok(())
    }

    pub fn snap_authoring_time(&self, t: f32, ignore: &[KeyRef], dest: Option<KeyRef>) -> f32 {
        snap_time(
            t,
            self.document().duration,
            self.snap,
            self.document(),
            ignore,
            dest,
        )
    }

    pub fn preview_batch_move(
        &mut self,
        origins: &[(KeyRef, f32)],
        grab_origin: f32,
        pointer_time: f32,
    ) -> Result<Vec<KeyRef>, String> {
        let dest = origins.first().map(|(r, _)| *r);
        let snapped = self.snap_authoring_time(
            pointer_time,
            &origins.iter().map(|(r, _)| *r).collect::<Vec<_>>(),
            dest,
        );
        let delta = snapped - grab_origin;
        let tx = self
            .transaction
            .as_mut()
            .ok_or_else(|| "no active edit transaction".to_string())?;
        tx.working = tx.before.clone();
        let moves: Vec<(KeyRef, f32)> = origins.iter().map(|(r, t)| (*r, t + delta)).collect();
        let inserted = tx.working.move_keys(&moves)?;
        self.selected_keys = inserted.iter().copied().collect();
        if let Some(first) = inserted.first() {
            self.selected_bone = Some(first.bone);
            self.selected_channel = first.kind;
            self.selected_key = tx.working.track(first.bone).and_then(|tr| {
                tr.channel(first.kind)
                    .iter()
                    .position(|k| times_equal(k.time, first.time()))
            });
        }
        self.sync_key_drafts();
        Ok(inserted)
    }

    pub fn set_transition_clip(&mut self, path: PathBuf) -> Result<(), String> {
        let doc = load_anim_file(&path)?;
        self.transition_b = Some((path, doc));
        self.last_error = None;
        Ok(())
    }

    pub fn advance_transition(&mut self, dt: f32) {
        if !self.transition_playing || !self.transition_enabled {
            return;
        }
        let dur = self.transition_duration.max(0.01);
        self.transition_alpha += dt / dur;
        if self.transition_alpha >= 1.0 {
            self.transition_alpha = 1.0;
            self.transition_playing = false;
        }
    }

    pub fn sync_drafts_from_document(&mut self) {
        self.duration_draft = format!("{:.2}", self.document.duration);
        self.sync_key_drafts();
        if self.playhead > self.document.duration {
            self.playhead = self.document.duration;
        }
    }

    pub fn sync_key_drafts(&mut self) {
        let Some(bone) = self.selected_bone else {
            self.key_time_draft.clear();
            self.key_value_draft.clear();
            return;
        };
        let Some(index) = self.selected_key else {
            self.key_time_draft.clear();
            self.key_value_draft.clear();
            return;
        };
        let Some(key) = self
            .document()
            .track(bone)
            .and_then(|tr| tr.channel(self.selected_channel).get(index))
            .copied()
        else {
            self.key_time_draft.clear();
            self.key_value_draft.clear();
            return;
        };
        self.key_time_draft = format!("{:.2}", key.time);
        self.key_value_draft = format!("{:.4}", key.value);
    }
}

const KEY_TIME_FALLBACK: f32 = 0.01;
