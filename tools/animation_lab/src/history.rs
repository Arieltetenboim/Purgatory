//! Command-based document history. Each entry is a labeled snapshot.

use crate::document::AnimDocument;

#[derive(Clone, Debug, PartialEq)]
pub struct HistoryEntry {
    pub label: String,
    pub document: AnimDocument,
}

#[derive(Clone, Debug)]
pub struct EditHistory {
    entries: Vec<HistoryEntry>,
    cursor: usize,
}

impl EditHistory {
    #[must_use]
    pub fn new(label: impl Into<String>, document: AnimDocument) -> Self {
        Self {
            entries: vec![HistoryEntry {
                label: label.into(),
                document,
            }],
            cursor: 0,
        }
    }

    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    #[must_use]
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    #[must_use]
    pub fn current(&self) -> &AnimDocument {
        &self.entries[self.cursor].document
    }

    /// Push a new committed document. If the cursor is not at the tip, redo is discarded.
    pub fn push(&mut self, label: impl Into<String>, document: AnimDocument) {
        if self.cursor + 1 < self.entries.len() {
            self.entries.truncate(self.cursor + 1);
        }
        self.entries.push(HistoryEntry {
            label: label.into(),
            document,
        });
        self.cursor = self.entries.len() - 1;
    }

    pub fn undo(&mut self) -> Option<&AnimDocument> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        Some(self.current())
    }

    pub fn redo(&mut self) -> Option<&AnimDocument> {
        if self.cursor + 1 >= self.entries.len() {
            return None;
        }
        self.cursor += 1;
        Some(self.current())
    }

    pub fn jump(&mut self, index: usize) -> Option<&AnimDocument> {
        if index >= self.entries.len() {
            return None;
        }
        self.cursor = index;
        Some(self.current())
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.cursor + 1 < self.entries.len()
    }
}
