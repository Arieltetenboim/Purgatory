//! Shaped, physical-size text for ordered framebuffer UI compositions.
use glyphon::cosmic_text::Align;
use glyphon::{
    Attrs, Buffer, Cache, Color, ColorMode, Family, FontSystem, Metrics, Resolution, Shaping,
    SwashCache, TextArea, TextAtlas, TextBounds, Viewport, Weight, Wrap,
};

const MAX_CHARS: usize = 4096;

#[cfg(test)]
#[path = "text_gpu_tests.rs"]
mod gpu_tests;

#[derive(Clone, Debug, PartialEq)]
pub struct TextContent(pub String);
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum FontFamily {
    #[default]
    Ui,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    /// Logical UI units. Only the text adapter converts these to physical pixels.
    pub font_size: f32,
    pub line_height: f32,
    pub family: FontFamily,
    /// Linear, straight-alpha RGBA.
    pub color: [f32; 4],
    pub alignment: Alignment,
}
impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_size: 24.0,
            line_height: 28.0,
            family: FontFamily::Ui,
            color: [1.0; 4],
            alignment: Alignment::Left,
        }
    }
}
impl TextStyle {
    pub(crate) fn at_size(font_size: f32, color: [f32; 4], alignment: Alignment) -> Self {
        Self {
            font_size,
            line_height: font_size * 1.2,
            color,
            alignment,
            ..Self::default()
        }
    }
    fn valid(self, scale: f32) -> bool {
        scale.is_finite()
            && scale > 0.0
            && self.font_size.is_finite()
            && self.font_size > 0.0
            && self.font_size <= 128.0
            && self.line_height.is_finite()
            && self.line_height > 0.0
            && (self.font_size * scale).is_finite()
            && (self.line_height * scale).is_finite()
            && self
                .color
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    }
}
/// Geometry is already in framebuffer pixels; it is never scaled by this adapter.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextBlock {
    pub content: TextContent,
    pub style: TextStyle,
    pub anchor: [f32; 2],
    pub max_width: Option<f32>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct TextMetrics {
    pub width: f32,
    pub height: f32,
    pub line_count: u32,
}

fn font_system() -> FontSystem {
    let mut db = glyphon::fontdb::Database::new();
    db.load_font_data(crate::assets::UI_FONT.to_vec());
    // Do not call load_system_fonts or FontSystem::new: installed fonts are not game assets.
    FontSystem::new_with_locale_and_db("en-US".into(), db)
}

/// Glyphon Accurate expects sRGB RGB bytes and converts them back to linear in its shader.
/// Alpha is coverage/opacity, not an sRGB channel.
fn glyphon_color(linear: [f32; 4]) -> Color {
    let channel = |v: f32| {
        let srgb = if v <= 0.0031308 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        };
        (srgb * 255.0).round() as u8
    };
    Color::rgba(
        channel(linear[0]),
        channel(linear[1]),
        channel(linear[2]),
        (linear[3] * 255.0).round() as u8,
    )
}

struct TextLayout {
    request: TextBlock,
    scale: f32,
    buffer: Buffer,
    metrics: TextMetrics,
    left: f32,
}
impl TextLayout {
    fn new(fonts: &mut FontSystem, request: TextBlock, scale: f32) -> Option<Self> {
        if !request.style.valid(scale)
            || !request.anchor.iter().all(|v| v.is_finite())
            || request
                .max_width
                .is_some_and(|w| !w.is_finite() || w <= 0.0)
            || request.content.0.is_empty()
        {
            return None;
        }
        let style = request.style;
        let mut buffer = Buffer::new(
            fonts,
            Metrics::new(style.font_size * scale, style.line_height * scale),
        );
        buffer.set_size(request.max_width, None);
        buffer.set_wrap(if request.max_width.is_some() {
            Wrap::WordOrGlyph
        } else {
            Wrap::None
        });
        buffer.set_text(
            &request.content.0,
            &Attrs::new()
                .family(Family::Name("DejaVu Sans"))
                .weight(Weight::BOLD),
            Shaping::Advanced,
            Some(Align::Left),
        );
        buffer.shape_until_scroll(fonts, false);
        let mut metrics = TextMetrics::default();
        for run in buffer.layout_runs() {
            metrics.width = metrics.width.max(run.line_w);
            metrics.height = metrics.height.max(run.line_top + run.line_height);
            metrics.line_count += 1;
        }
        // Align inside a shared width, then place that box around the supplied anchor.
        // Cosmic owns all per-line positioning, including bidi and shaping.
        let width = request.max_width.unwrap_or(metrics.width).max(0.001);
        let (align, shift) = match style.alignment {
            Alignment::Left => (Align::Left, 0.0),
            Alignment::Center => (Align::Center, width * 0.5),
            Alignment::Right => (Align::Right, width),
        };
        if style.alignment != Alignment::Left {
            buffer.set_size(Some(width), None);
            for line in &mut buffer.lines {
                line.set_align(Some(align));
            }
            buffer.shape_until_scroll(fonts, false);
        }
        let left = request.anchor[0] - shift;
        Some(Self {
            request,
            scale,
            buffer,
            metrics,
            left,
        })
    }
    fn area(&self) -> TextArea<'_> {
        TextArea {
            buffer: &self.buffer,
            left: self.left,
            top: self.request.anchor[1],
            scale: 1.0,
            bounds: TextBounds::default(),
            default_color: glyphon_color(self.request.style.color),
            custom_glyphs: &[],
        }
    }
}
struct TextBatch {
    renderer: glyphon::TextRenderer,
    layouts: Vec<TextLayout>,
    ready: bool,
}
pub struct TextRenderer {
    fonts: FontSystem,
    swash: SwashCache,
    atlas: TextAtlas,
    viewport: Viewport,
    batches: Vec<TextBatch>,
    active: usize,
}
impl TextRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let cache = Cache::new(device);
        Self {
            fonts: font_system(),
            swash: SwashCache::new(),
            atlas: TextAtlas::with_color_mode(device, queue, &cache, format, ColorMode::Accurate),
            viewport: Viewport::new(device, &cache),
            batches: Vec::new(),
            active: 0,
        }
    }
    /// Called once before preparing any composition. Never trim between batches.
    pub(crate) fn begin_frame(&mut self, queue: &wgpu::Queue, viewport: [u32; 2]) {
        self.atlas.trim();
        self.active = 0;
        self.viewport.update(
            queue,
            Resolution {
                width: viewport[0],
                height: viewport[1],
            },
        );
    }
    pub(crate) fn prepare_blocks(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        blocks: &[TextBlock],
        pixels_per_unit: f32,
    ) -> Result<usize, String> {
        let index = self.active;
        self.active += 1;
        if index == self.batches.len() {
            self.batches.push(TextBatch {
                renderer: glyphon::TextRenderer::new(
                    &mut self.atlas,
                    device,
                    Default::default(),
                    None,
                ),
                layouts: Vec::new(),
                ready: false,
            });
        }
        let batch = &mut self.batches[index];
        batch.ready = false;
        let mut old = std::mem::take(&mut batch.layouts).into_iter();
        let mut remaining = MAX_CHARS;
        for block in blocks.iter().take(MAX_CHARS) {
            if remaining == 0 {
                break;
            }
            let content: String = block.content.0.chars().take(remaining).collect();
            remaining -= content.chars().count();
            let request = TextBlock {
                content: TextContent(content),
                style: block.style,
                anchor: block.anchor,
                max_width: block.max_width,
            };
            let previous = old.next();
            if let Some(previous) =
                previous.filter(|l| l.request == request && l.scale == pixels_per_unit)
            {
                batch.layouts.push(previous);
            } else if let Some(layout) = TextLayout::new(&mut self.fonts, request, pixels_per_unit)
            {
                batch.layouts.push(layout);
            }
        }
        batch
            .renderer
            .prepare(
                device,
                queue,
                &mut self.fonts,
                &mut self.atlas,
                &self.viewport,
                batch.layouts.iter().map(TextLayout::area),
                &mut self.swash,
            )
            .map_err(|e| e.to_string())?;
        batch.ready = true;
        Ok(index)
    }
    #[cfg_attr(not(test), allow(dead_code))] // Measurement contract; existing panels retain authored geometry.
    pub(crate) fn metrics(&self, batch: usize) -> impl Iterator<Item = TextMetrics> + '_ {
        self.batches[batch].layouts.iter().map(|l| l.metrics)
    }
    pub fn draw(&self, index: usize, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        let Some(batch) = self.batches.get(index).filter(|b| b.ready) else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("production-text"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if let Err(error) = batch
            .renderer
            .render(&self.atlas, &self.viewport, &mut pass)
        {
            tracing::error!(%error, "text render failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(text: &str, size: f32) -> TextBlock {
        TextBlock {
            content: TextContent(text.into()),
            style: TextStyle::at_size(size, [1.0; 4], Alignment::Left),
            anchor: [20.0, 20.0],
            max_width: None,
        }
    }
    fn layout(text: &str) -> TextLayout {
        TextLayout::new(&mut font_system(), block(text, 16.0), 1.0).unwrap()
    }
    #[test]
    fn explicit_empty_lines_and_measurement() {
        let one = layout("Hello");
        let many = layout("Hello\n\nworld\n");
        assert_eq!(many.metrics.line_count, 4);
        assert!((many.metrics.height - one.metrics.height * 4.0).abs() < 0.01);
        assert_eq!(
            one.metrics.width,
            one.buffer.layout_runs().next().unwrap().line_w
        );
    }
    #[test]
    fn wrapping_and_alignment_are_shaped_layout() {
        let mut b = block("hello world longwordlongword", 16.0);
        b.max_width = Some(70.0);
        for alignment in [Alignment::Left, Alignment::Center, Alignment::Right] {
            b.style.alignment = alignment;
            let l = TextLayout::new(&mut font_system(), b.clone(), 1.0).unwrap();
            assert!(l.metrics.line_count > 1);
            assert!(l.metrics.width <= 70.01);
            for run in l.buffer.layout_runs() {
                let x = run.glyphs.first().unwrap().x + l.left;
                let expected = match alignment {
                    Alignment::Left => 20.0,
                    Alignment::Center => 20.0 - run.line_w * 0.5,
                    Alignment::Right => 20.0 - run.line_w,
                };
                assert!(
                    (x - expected).abs() < 0.1,
                    "{alignment:?}: {x} != {expected}"
                );
            }
        }
    }
    #[test]
    fn invalid_and_empty_inputs_do_not_layout() {
        let mut fonts = font_system();
        assert!(TextLayout::new(&mut fonts, block("", 16.0), 1.0).is_none());
        for size in [0.0, -1.0, f32::NAN, f32::INFINITY, 129.0] {
            assert!(TextLayout::new(&mut fonts, block("x", size), 1.0).is_none());
        }
        for scale in [0.0, -1.0, f32::NAN] {
            assert!(TextLayout::new(&mut fonts, block("x", 16.0), scale).is_none());
        }
        let mut b = block("x", 16.0);
        b.style.color[0] = 2.0;
        assert!(TextLayout::new(&mut fonts, b, 1.0).is_none());
    }
    #[test]
    fn effective_size_enters_glyph_cache_key_once() {
        let mut fonts = font_system();
        for size in [12.0, 13.0, 14.0, 16.0, 18.0, 24.0, 32.0] {
            let a = TextLayout::new(&mut fonts, block("A", size), 1.0).unwrap();
            let b = TextLayout::new(&mut fonts, block("A", size), 1.25 * 1.5).unwrap();
            let key = |l: &TextLayout| {
                l.buffer.layout_runs().next().unwrap().glyphs[0]
                    .physical((0.0, 0.0), 1.0)
                    .cache_key
            };
            assert_eq!(f32::from_bits(key(&a).font_size_bits), size);
            assert_eq!(f32::from_bits(key(&b).font_size_bits), size * 1.875);
            assert_ne!(key(&a), key(&b));
        }
    }
    #[test]
    fn advanced_shaping_preserves_non_ascii_clusters() {
        let composed = layout("é");
        let decomposed = layout("e\u{301}");
        let ids = |l: &TextLayout| {
            l.buffer
                .layout_runs()
                .flat_map(|r| r.glyphs.iter().map(|g| g.glyph_id))
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&composed), ids(&decomposed));
        assert_eq!(ids(&decomposed).len(), 1);
        assert_ne!(ids(&composed), ids(&layout("?")));
        assert_eq!(
            decomposed.buffer.layout_runs().next().unwrap().glyphs[0].end,
            3
        );
    }
    #[test]
    fn linear_color_conversion_preserves_alpha() {
        assert_eq!(glyphon_color([1.0; 4]), Color::rgba(255, 255, 255, 255));
        assert_eq!(
            glyphon_color([0.0, 0.5, 1.0, 0.5]),
            Color::rgba(0, 188, 255, 128)
        );
    }
    #[test]
    fn fixed_choice_rows_report_scaled_typography_overflow_without_shrinking() {
        let l = TextLayout::new(&mut font_system(), block("Choice", 16.0), 1.875).unwrap();
        assert!(
            l.metrics.height > 28.0,
            "the existing physical choice row is 28px"
        );
        assert_eq!(l.buffer.metrics().font_size, 30.0);
    }
}
