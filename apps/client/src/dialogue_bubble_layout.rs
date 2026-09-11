//! Screen-space coordination for simultaneous NPC and player dialogue bubbles.
//!
//! Individual bubble modules still own their own skin and contents. This
//! module only assigns non-overlapping horizontal columns when both speakers
//! are visible.

use crate::renderer::{Camera, PixelViewport};

const SAFE_MARGIN_PX: f32 = 8.0;
const COLUMN_GAP_PX: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BubbleColumn {
    pub min_x: f32,
    pub max_x: f32,
}

impl BubbleColumn {
    #[must_use]
    pub(crate) fn full(viewport: PixelViewport) -> Self {
        Self {
            min_x: viewport.x as f32 + SAFE_MARGIN_PX,
            max_x: viewport.x as f32 + viewport.width as f32 - SAFE_MARGIN_PX,
        }
    }

    #[must_use]
    pub(crate) fn width(self) -> f32 {
        (self.max_x - self.min_x).max(1.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DialogueBubbleColumns {
    pub player: BubbleColumn,
    pub npc: BubbleColumn,
}

#[must_use]
pub(crate) fn dialogue_bubble_columns(
    player_world: [f32; 2],
    npc_world: [f32; 2],
    camera: Camera,
    viewport: PixelViewport,
) -> DialogueBubbleColumns {
    let player_x = viewport.ndc_to_px(camera.world_to_ndc(player_world))[0];
    let npc_x = viewport.ndc_to_px(camera.world_to_ndc(npc_world))[0];
    let full = BubbleColumn::full(viewport);
    let min_divider = full.min_x + COLUMN_GAP_PX * 0.5 + 1.0;
    let max_divider = full.max_x - COLUMN_GAP_PX * 0.5 - 1.0;
    let divider = if min_divider <= max_divider {
        ((player_x + npc_x) * 0.5).clamp(min_divider, max_divider)
    } else {
        (full.min_x + full.max_x) * 0.5
    };
    let left = BubbleColumn {
        min_x: full.min_x,
        max_x: (divider - COLUMN_GAP_PX * 0.5).max(full.min_x + 1.0),
    };
    let right = BubbleColumn {
        min_x: (divider + COLUMN_GAP_PX * 0.5).min(full.max_x - 1.0),
        max_x: full.max_x,
    };
    if player_x <= npc_x {
        DialogueBubbleColumns {
            player: left,
            npc: right,
        }
    } else {
        DialogueBubbleColumns {
            player: right,
            npc: left,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::choice_bubble::layout_choice_bubble_in_column;
    use crate::speech_bubble::{SpeechBubbleSpeaker, layout_speech_bubble_in_column};
    use purgatory_content::DialoguePresentationChoice;

    fn viewport() -> PixelViewport {
        PixelViewport {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
        }
    }

    #[test]
    fn close_speakers_receive_non_overlapping_columns() {
        let columns = dialogue_bubble_columns(
            [0.0, 0.0],
            [0.05, 0.0],
            Camera::footnote_test_dev(),
            viewport(),
        );
        assert!(columns.player.max_x < columns.npc.min_x);
        assert!(columns.npc.min_x - columns.player.max_x >= COLUMN_GAP_PX);
    }

    #[test]
    fn columns_follow_speaker_order_and_swap_when_they_cross() {
        let columns = dialogue_bubble_columns(
            [2.0, 0.0],
            [-2.0, 0.0],
            Camera::footnote_test_dev(),
            viewport(),
        );
        assert!(columns.npc.max_x < columns.player.min_x);
    }

    #[test]
    fn close_speaker_bubbles_are_laid_out_side_by_side() {
        let viewport = viewport();
        let camera = Camera::footnote_test_dev();
        let player = [0.0, 0.0];
        let npc = [0.05, 0.0];
        let columns = dialogue_bubble_columns(player, npc, camera, viewport);
        let npc_bubble = layout_speech_bubble_in_column(
            "NPC Beat",
            npc,
            camera,
            viewport,
            columns.npc,
            SpeechBubbleSpeaker::Npc,
        );
        let choice_bubble = layout_choice_bubble_in_column(
            &[DialoguePresentationChoice {
                text: "Player choice".into(),
            }],
            0,
            player,
            camera,
            viewport,
            columns.player,
        );
        assert!(choice_bubble.rects[0].max[0] < npc_bubble.hit_bounds.min[0]);
    }
}
