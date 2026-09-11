//! Player-choice presentation consumer.
//!
//! This module owns only choice layout and hit regions. Text rasterization is
//! provided by the generic renderer Text system; dialogue authority stays in
//! `dialogue_runtime` and the server.

use purgatory_content::DialoguePresentationChoice;

use crate::dialogue_bubble_layout::BubbleColumn;
use crate::renderer::{
    Camera, PixelViewport, TextAlignment, TextBlock, TextContent, TextStyle, UiRect,
};

const WIDTH_PX: f32 = 420.0;
const ROW_HEIGHT_PX: f32 = 28.0;
const ROW_GAP_PX: f32 = 8.0;
const PADDING_PX: f32 = 12.0;
const BORDER_PX: f32 = 3.0;
const WORLD_ANCHOR_Y: f32 = 2.2;
const TEXT_TOP_INSET_PX: f32 = 5.0;

pub(crate) struct ChoiceBubbleLayout {
    pub rects: Vec<UiRect>,
    pub texts: Vec<TextBlock>,
    pub choice_hits: Vec<UiRect>,
}

#[must_use]
#[cfg(test)]
pub(crate) fn layout_choice_bubble(
    choices: &[DialoguePresentationChoice],
    selected: usize,
    player_world: [f32; 2],
    camera: Camera,
    viewport: PixelViewport,
) -> ChoiceBubbleLayout {
    layout_choice_bubble_in_column(
        choices,
        selected,
        player_world,
        camera,
        viewport,
        BubbleColumn::full(viewport),
    )
}

#[must_use]
pub(crate) fn layout_choice_bubble_in_column(
    choices: &[DialoguePresentationChoice],
    selected: usize,
    player_world: [f32; 2],
    camera: Camera,
    viewport: PixelViewport,
    column: BubbleColumn,
) -> ChoiceBubbleLayout {
    let anchor = viewport
        .ndc_to_px(camera.world_to_ndc([player_world[0], player_world[1] + WORLD_ANCHOR_Y]));
    let width = WIDTH_PX.min(column.width());
    let row_gaps = choices.len().saturating_sub(1) as f32 * ROW_GAP_PX;
    let height = PADDING_PX * 2.0 + ROW_HEIGHT_PX * choices.len() as f32 + row_gaps;
    let min_x = column.min_x;
    let max_x = column.max_x - width;
    let min_y = viewport.y as f32 + 8.0;
    let max_y = viewport.y as f32 + viewport.height as f32 - 8.0 - height;
    let panel_min = [
        (anchor[0] - width * 0.5).clamp(min_x, max_x.max(min_x)),
        (anchor[1] - height - 20.0).clamp(min_y, max_y.max(min_y)),
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
            let min_y = panel_min[1] + PADDING_PX + index as f32 * (ROW_HEIGHT_PX + ROW_GAP_PX);
            UiRect {
                min: [panel_min[0] + PADDING_PX, min_y],
                max: [panel_max[0] - PADDING_PX, min_y + ROW_HEIGHT_PX],
                color: [0.0; 4],
            }
        })
        .collect();
    for (index, hit) in choice_hits.iter().enumerate() {
        rects.push(UiRect {
            min: hit.min,
            max: hit.max,
            color: if index == selected {
                [0.40, 0.33, 0.52, 1.0]
            } else {
                [0.24, 0.22, 0.29, 0.98]
            },
        });
    }
    let texts = choices
        .iter()
        .enumerate()
        .zip(choice_hits.iter())
        .map(|((index, choice), hit)| TextBlock {
            content: TextContent(format!(
                "{}{}",
                if index == selected { "> " } else { "  " },
                choice.text
            )),
            style: TextStyle {
                font_size: 16.0,
                color: [0.96, 0.93, 0.84, 1.0],
                alignment: TextAlignment::Left,
            },
            anchor: [hit.min[0] + 4.0, hit.min[1] + TEXT_TOP_INSET_PX],
            max_width: Some((hit.max[0] - hit.min[0] - 8.0).max(1.0)),
        })
        .collect();
    ChoiceBubbleLayout {
        rects,
        texts,
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
        assert_eq!(layout.texts.len(), 2);
        assert_eq!(
            layout.choice_hits[1].min[1] - layout.choice_hits[0].max[1],
            ROW_GAP_PX
        );
        assert_eq!(layout.rects[2].max, layout.choice_hits[0].max);
        assert_eq!(layout.rects[3].min, layout.choice_hits[1].min);
        assert_ne!(layout.rects[2].color, layout.rects[3].color);
        for (text, hit) in layout.texts.iter().zip(&layout.choice_hits) {
            assert!(hit.contains(text.anchor));
        }
        assert_eq!(layout.texts[0].content.0, "  One");
        assert_eq!(layout.texts[1].content.0, "> Two");
    }
}
