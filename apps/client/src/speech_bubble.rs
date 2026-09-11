//! SpeechBubble presentation consumer.
//!
//! Bubble skin, world anchoring, viewport clamping, and hit bounds live here.
//! Glyphs, font metrics, and wrapping remain in the generic Text system.

use crate::dialogue_bubble_layout::BubbleColumn;
use crate::renderer::{
    Camera, PixelViewport, TextAlignment, TextBlock, TextContent, TextStyle, UiRect,
};

const MAX_WIDTH_PX: f32 = 460.0;
const HEIGHT_PX: f32 = 112.0;
const SAFE_MARGIN_PX: f32 = 8.0;
const BORDER_PX: f32 = 3.0;
const PADDING_X_PX: f32 = 16.0;
const PADDING_Y_PX: f32 = 12.0;
const TAIL_WIDTH_PX: f32 = 14.0;
const TAIL_HEIGHT_PX: f32 = 12.0;
const WORLD_ANCHOR_Y: f32 = 2.35;

pub(crate) struct SpeechBubbleLayout {
    pub rects: Vec<UiRect>,
    pub text: TextBlock,
    pub hit_bounds: UiRect,
}

#[cfg(test)]
impl SpeechBubbleLayout {
    #[must_use]
    pub(crate) fn contains(&self, cursor: [f32; 2]) -> bool {
        self.hit_bounds.contains(cursor)
    }
}

#[must_use]
#[cfg(test)]
pub(crate) fn layout_speech_bubble(
    content: &str,
    target_world: [f32; 2],
    camera: Camera,
    viewport: PixelViewport,
) -> SpeechBubbleLayout {
    layout_speech_bubble_in_column(
        content,
        target_world,
        camera,
        viewport,
        BubbleColumn::full(viewport),
        SpeechBubbleSpeaker::Npc,
    )
}

#[must_use]
pub(crate) fn layout_speech_bubble_in_column(
    content: &str,
    target_world: [f32; 2],
    camera: Camera,
    viewport: PixelViewport,
    column: BubbleColumn,
    speaker: SpeechBubbleSpeaker,
) -> SpeechBubbleLayout {
    let target_px = viewport
        .ndc_to_px(camera.world_to_ndc([target_world[0], target_world[1] + WORLD_ANCHOR_Y]));
    let safe_width = column.width();
    let safe_height = (viewport.height as f32 - SAFE_MARGIN_PX * 2.0).max(1.0);
    let width = MAX_WIDTH_PX.min(safe_width);
    let height = HEIGHT_PX.min((safe_height - TAIL_HEIGHT_PX).max(1.0));
    let min_x = column.min_x;
    let max_x = column.max_x - width;
    let min_y = viewport.y as f32 + SAFE_MARGIN_PX;
    let max_y =
        viewport.y as f32 + viewport.height as f32 - SAFE_MARGIN_PX - height - TAIL_HEIGHT_PX;
    let panel_x = (target_px[0] - width * 0.5).clamp(min_x, max_x.max(min_x));
    let panel_y = (target_px[1] - height - TAIL_HEIGHT_PX - 18.0).clamp(min_y, max_y.max(min_y));
    let panel_min = [panel_x, panel_y];
    let panel_max = [panel_x + width, panel_y + height];
    let tail_x = target_px[0].clamp(panel_min[0] + TAIL_WIDTH_PX, panel_max[0] - TAIL_WIDTH_PX);

    let palette = speaker.palette();
    let border = UiRect {
        min: panel_min,
        max: panel_max,
        color: palette.border,
    };
    let fill = UiRect {
        min: [panel_min[0] + BORDER_PX, panel_min[1] + BORDER_PX],
        max: [panel_max[0] - BORDER_PX, panel_max[1] - BORDER_PX],
        color: palette.fill,
    };
    let tail_border = UiRect {
        min: [tail_x - TAIL_WIDTH_PX * 0.5, panel_max[1]],
        max: [tail_x + TAIL_WIDTH_PX * 0.5, panel_max[1] + TAIL_HEIGHT_PX],
        color: border.color,
    };
    let tail_fill = UiRect {
        min: [tail_border.min[0] + BORDER_PX, tail_border.min[1]],
        max: [
            tail_border.max[0] - BORDER_PX,
            tail_border.max[1] - BORDER_PX,
        ],
        color: fill.color,
    };
    let hit_bounds = UiRect {
        min: panel_min,
        max: [panel_max[0], panel_max[1] + TAIL_HEIGHT_PX],
        color: [0.0; 4],
    };
    let text = TextBlock {
        content: TextContent(content.to_owned()),
        style: TextStyle {
            font_size: 17.0,
            color: palette.text,
            alignment: TextAlignment::Left,
        },
        anchor: [panel_min[0] + PADDING_X_PX, panel_min[1] + PADDING_Y_PX],
        max_width: Some((width - PADDING_X_PX * 2.0).max(1.0)),
    };
    SpeechBubbleLayout {
        rects: vec![border, fill, tail_border, tail_fill],
        text,
        hit_bounds,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SpeechBubbleSpeaker {
    Npc,
    Player,
}

#[derive(Clone, Copy)]
struct SpeechBubblePalette {
    border: [f32; 4],
    fill: [f32; 4],
    text: [f32; 4],
}

impl SpeechBubbleSpeaker {
    fn palette(self) -> SpeechBubblePalette {
        match self {
            Self::Npc => SpeechBubblePalette {
                border: [0.08, 0.09, 0.12, 0.96],
                fill: [0.94, 0.91, 0.82, 0.98],
                text: [0.08, 0.07, 0.06, 1.0],
            },
            Self::Player => SpeechBubblePalette {
                border: [0.05, 0.16, 0.22, 0.98],
                fill: [0.64, 0.82, 0.90, 0.98],
                text: [0.03, 0.10, 0.14, 1.0],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bubble_clamps_to_gameplay_viewport_and_exposes_matching_hit_bounds() {
        let camera = Camera::footnote_test_dev();
        let viewport = PixelViewport {
            x: 100,
            y: 50,
            width: 800,
            height: 450,
        };
        for target in [[-100.0, 100.0], [100.0, 100.0], [0.0, -100.0]] {
            let bubble = layout_speech_bubble("hello", target, camera, viewport);
            assert!(bubble.hit_bounds.min[0] >= 108.0);
            assert!(bubble.hit_bounds.max[0] <= 892.0);
            assert!(bubble.hit_bounds.min[1] >= 58.0);
            assert!(bubble.hit_bounds.max[1] <= 492.0);
            assert!(bubble.contains(bubble.hit_bounds.min));
            assert!(!bubble.contains([99.0, 49.0]));
        }
    }

    #[test]
    fn bubble_passes_raw_content_and_width_to_text_system() {
        let bubble = layout_speech_bubble(
            "one authored line",
            [0.0, 0.0],
            Camera::footnote_test_dev(),
            PixelViewport {
                x: 0,
                y: 0,
                width: 1280,
                height: 720,
            },
        );
        assert_eq!(bubble.text.content.0, "one authored line");
        assert!(bubble.text.max_width.is_some_and(|width| width > 300.0));
    }

    #[test]
    fn player_and_npc_bubbles_use_distinct_skins() {
        let viewport = PixelViewport {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
        };
        let npc = layout_speech_bubble("npc", [0.0, 0.0], Camera::footnote_test_dev(), viewport);
        let player = layout_speech_bubble_in_column(
            "player",
            [0.0, 0.0],
            Camera::footnote_test_dev(),
            viewport,
            BubbleColumn::full(viewport),
            SpeechBubbleSpeaker::Player,
        );
        assert_ne!(npc.rects[1].color, player.rects[1].color);
        assert_ne!(npc.text.style.color, player.text.style.color);
    }
}
