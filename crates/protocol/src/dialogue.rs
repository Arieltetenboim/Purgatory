//! Protocol v25 authoritative dialogue line identity.
//!
//! The client may request only progression of its current interaction session.
//! Text, authored actions, and next-beat authority never travel in client input.

use purgatory_common::ContentId;

use crate::snapshot::WireEntityId;

pub const DIALOGUE_ADVANCE_BYTES: usize = 4;
pub const DIALOGUE_ACTIVE_LINE_BYTES: usize = 4 + 8 + 4 + 4 + 4;

/// Client → server: advance the dialogue associated with this interaction
/// session. The server validates ownership and current progression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DialogueAdvance {
    pub session_id: u32,
}

/// Server → client: semantic identity of the currently visible NPC line.
/// The client resolves presentation text from its client-safe content
/// projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServerDialogueLine {
    pub session_id: u32,
    pub target: WireEntityId,
    pub npc_content_id: ContentId,
    pub beat_index: u32,
    pub line_index: u32,
}
