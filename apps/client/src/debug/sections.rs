//! Collapsible debug-overlay sections. Presentation only.

use std::collections::HashMap;

/// Open/closed flags for overlay sections. Lives as long as the overlay.
#[derive(Clone, Debug, Default)]
pub struct DebugSectionMap {
    open: HashMap<&'static str, bool>,
}

impl DebugSectionMap {
    #[must_use]
    pub fn is_open(&self, id: &'static str, default_open: bool) -> bool {
        self.open.get(id).copied().unwrap_or(default_open)
    }

    pub fn set(&mut self, id: &'static str, open: bool) {
        self.open.insert(id, open);
    }

    pub fn set_all(&mut self, ids: &[&'static str], open: bool) {
        for id in ids {
            self.open.insert(*id, open);
        }
    }
}

pub fn draw_expand_collapse(
    ui: &mut egui::Ui,
    sections: &mut DebugSectionMap,
    ids: &[&'static str],
) {
    if ids.len() < 2 {
        return;
    }
    ui.horizontal(|ui| {
        if ui.small_button("Expand all").clicked() {
            sections.set_all(ids, true);
        }
        if ui.small_button("Collapse all").clicked() {
            sections.set_all(ids, false);
        }
    });
}

/// Clickable `▼` / `▶` header. Collapsed headers may show `summary`.
/// Returns whether the section body should be drawn.
#[must_use]
pub fn debug_section(
    ui: &mut egui::Ui,
    sections: &mut DebugSectionMap,
    id: &'static str,
    default_open: bool,
    title: &str,
    summary: Option<&str>,
) -> bool {
    let open = sections.is_open(id, default_open);
    let arrow = if open { "▼" } else { "▶" };
    let text = match (open, summary) {
        (false, Some(summary)) if !summary.is_empty() => {
            format!("{arrow} {title} — {summary}")
        }
        _ => format!("{arrow} {title}"),
    };
    let response = ui
        .add(egui::Label::new(egui::RichText::new(text).strong()).sense(egui::Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        sections.set(id, !open);
        !open
    } else {
        open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_id_uses_default_until_set() {
        let mut sections = DebugSectionMap::default();
        assert!(sections.is_open("net.prediction", true));
        assert!(!sections.is_open("net.connection", false));
        sections.set("net.prediction", false);
        assert!(!sections.is_open("net.prediction", true));
        sections.set("net.prediction", true);
        assert!(sections.is_open("net.prediction", false));
    }

    #[test]
    fn set_all_overrides_defaults() {
        let mut sections = DebugSectionMap::default();
        let ids = ["net.prediction", "net.impairment", "net.connection"];
        sections.set_all(&ids, false);
        assert!(!sections.is_open("net.prediction", true));
        assert!(!sections.is_open("net.connection", true));
        sections.set_all(&ids, true);
        assert!(sections.is_open("net.connection", false));
    }
}
