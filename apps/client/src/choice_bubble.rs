//! Player-choice presentation consumer.
//!
//! This module owns only choice layout and hit regions. Text rasterization is
//! provided by the generic renderer Text system; dialogue authority stays in
//! `dialogue_runtime` and the server.

use purgatory_content::DialoguePresentationChoice;

use crate::renderer::{
    Camera, PixelViewport, TextAlignment, TextBlock, TextContent, TextStyle, UiRect,
};

const WIDTH_PX: f32 = 420.0;
const ROW_HEIGHT_PX: f32 = 30.0;
const PADDING_PX: f32 = 12.0;
const BORDER_PX: f32 = 3.0;
const SAFE_MARGIN_PX: f32 = 8.0;
const WORLD_ANCHOR_Y: f32 = 2.2;
const NPC_BUBBLE_CLEARANCE_PX: f32 = 132.0;

pub(crate) struct ChoiceBubbleLayout {
    pub rects: Vec<UiRect>,
    pub text: TextBlock,
    pub choice_hits: Vec<UiRect>,
}

#[must_use]
pub(crate) fn layout_choice_bubble(
    choices: &[DialoguePresentationChoice],
    selected: usize,
    player_world: [f32; 2],
    camera: Camera,
    viewport: PixelViewport,
) -> ChoiceBubbleLayout {
    let anchor = viewport
        .ndc_to_px(camera.world_to_ndc([player_world[0], player_world[1] + WORLD_ANCHOR_Y]));
    let width = WIDTH_PX.min((viewport.width as f32 - SAFE_MARGIN_PX * 2.0).max(1.0));
    let height = PADDING_PX * 2.0 + ROW_HEIGHT_PX * choices.len() as f32;
    let min_x = viewport.x as f32 + SAFE_MARGIN_PX;
    let max_x = viewport.x as f32 + viewport.width as f32 - SAFE_MARGIN_PX - width;
    let min_y = viewport.y as f32 + SAFE_MARGIN_PX;
    let max_y = viewport.y as f32 + viewport.height as f32 - SAFE_MARGIN_PX - height;
    let panel_min = [
        (anchor[0] - width * 0.5).clamp(min_x, max_x.max(min_x)),
        (anchor[1] - height - 20.0 - NPC_BUBBLE_CLEARANCE_PX).clamp(min_y, max_y.max(min_y)),
    ];
    let panel_max = [panel_min[0] + width, panel_min[1] + height];
    let mut rects = vec![
        UiRect {
            min: panel_min,
            max: panel_max,
            color: [0.08, 0.09, 0.12, 0.96],
        },
        UiRect {
            min: [panel_min[0] + BORDER_PX, panel_min[1] + BORDER_PX],
            max: [panel_max[0] - BORDER_PX, panel_max[1] - BORDER_PX],
            color: [0.20, 0.18, 0.24, 0.98],
        },
    ];
    let choice_hits: Vec<_> = choices
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let min_y = panel_min[1] + PADDING_PX + index as f32 * ROW_HEIGHT_PX;
            UiRect {
                min: [panel_min[0] + PADDING_PX, min_y],
                max: [panel_max[0] - PADDING_PX, min_y + ROW_HEIGHT_PX],
                color: [0.0; 4],
            }
        })
        .collect();
    if let Some(hit) = choice_hits.get(selected) {
        rects.push(UiRect {
            min: hit.min,
            max: hit.max,
            color: [0.36, 0.30, 0.46, 1.0],
        });
    }
    let content = choices
        .iter()
        .enumerate()
        .map(|(index, choice)| {
            format!(
                "{}{}",
                if index == selected { "> " } else { "  " },
                choice.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    ChoiceBubbleLayout {
        rects,
        text: TextBlock {
            content: TextContent(content),
            style: TextStyle {
                font_size: 16.0,
                color: [0.96, 0.93, 0.84, 1.0],
                alignment: TextAlignment::Left,
            },
            anchor: [panel_min[0] + PADDING_PX, panel_min[1] + PADDING_PX + 4.0],
            max_width: Some((width - PADDING_PX * 2.0).max(1.0)),
        },
        choice_hits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choice_rows_share_the_panel_and_have_distinct_hit_regions() {
        let choices = vec![
            DialoguePresentationChoice { text: "One".into() },
            DialoguePresentationChoice { text: "Two".into() },
        ];
        let layout = layout_choice_bubble(
            &choices,
            1,
            [0.0, 0.0],
            Camera::footnote_test_dev(),
            PixelViewport {
                x: 0,
                y: 0,
                width: 1280,
                height: 720,
            },
        );
        assert_eq!(layout.choice_hits.len(), 2);
        assert!(layout.choice_hits[0].max[1] <= layout.choice_hits[1].min[1]);
        assert!(layout.text.content.0.contains("> Two"));
    }
}
