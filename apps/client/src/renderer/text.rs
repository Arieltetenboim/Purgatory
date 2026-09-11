//! Text v0: plain strings, manual newlines, one font, framebuffer-pixel UI.
use ab_glyph::{Font, FontRef, ScaleFont};
use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;

const RASTER_SIZE: f32 = 48.0;
const CELL: u32 = 64;
const ATLAS_SIZE: u32 = 1024;
const MAX_GLYPHS: usize = 4096;

pub struct TextContent(pub String);
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Alignment {
    #[default]
    Left,
    #[cfg_attr(not(test), allow(dead_code))] // Reusable Text v0 API; N10c uses Left.
    Center,
    #[cfg_attr(not(test), allow(dead_code))] // Reusable Text v0 API; N10c uses Left.
    Right,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub font_size: f32,
    pub color: [f32; 4],
    pub alignment: Alignment,
}
impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_size: 24.0,
            color: [1.0; 4],
            alignment: Alignment::Left,
        }
    }
}
impl TextStyle {
    fn valid(self) -> bool {
        self.font_size.is_finite()
            && self.font_size > 0.0
            && self.font_size <= 128.0
            && self
                .color
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    }
}

/// One generic screen-space text request. Wrapping is owned here rather than
/// by SpeechBubble or another consumer.
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
#[derive(Clone, Copy, Debug, PartialEq)]
struct CachedGlyph {
    cell: u32,
    advance: f32,
    bounds: [f32; 4],
}
/// CPU metadata and pending coverage uploads for a single fixed GPU atlas.
struct GlyphAtlas {
    font: FontRef<'static>,
    glyphs: HashMap<char, CachedGlyph>,
    pending: Vec<(u32, Vec<u8>)>,
}
impl GlyphAtlas {
    fn new() -> Result<Self, String> {
        Ok(Self {
            font: FontRef::try_from_slice(crate::assets::UI_FONT).map_err(|e| e.to_string())?,
            glyphs: HashMap::new(),
            pending: Vec::new(),
        })
    }
    fn resolve(&self, ch: char) -> char {
        if ((' '..='~').contains(&ch) || ch == '—') && self.font.glyph_id(ch).0 != 0 {
            ch
        } else {
            '?'
        }
    }
    fn get(&mut self, ch: char) -> CachedGlyph {
        let ch = self.resolve(ch);
        if let Some(glyph) = self.glyphs.get(&ch) {
            return *glyph;
        }
        let scaled = self.font.as_scaled(RASTER_SIZE);
        let id = scaled.glyph_id(ch);
        let cell = self.glyphs.len() as u32;
        let mut cached = CachedGlyph {
            cell,
            advance: scaled.h_advance(id),
            bounds: [0.0; 4],
        };
        let mut pixels = vec![0; (CELL * CELL) as usize];
        if let Some(outline) = self.font.outline_glyph(id.with_scale(RASTER_SIZE)) {
            let b = outline.px_bounds();
            let width = b.width() as u32;
            let height = b.height() as u32;
            // Guard asset changes; a cell always has a transparent border.
            if width <= CELL - 2 && height <= CELL - 2 {
                cached.bounds = [b.min.x, b.min.y, b.width(), b.height()];
                outline.draw(|x, y, coverage| {
                    pixels[((y + 1) * CELL + x + 1) as usize] = (coverage * 255.0).round() as u8
                });
            }
        }
        self.pending.push((cell, pixels));
        self.glyphs.insert(ch, cached);
        cached
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}
struct TextLayout {
    vertices: Vec<Vertex>,
    metrics: TextMetrics,
}
impl TextLayout {
    #[cfg_attr(not(test), allow(dead_code))]
    fn new(
        content: &TextContent,
        style: TextStyle,
        anchor: [f32; 2],
        viewport: [u32; 2],
        atlas: &mut GlyphAtlas,
    ) -> Self {
        Self::with_max_width(content, style, anchor, viewport, None, atlas)
    }

    fn with_max_width(
        content: &TextContent,
        style: TextStyle,
        anchor: [f32; 2],
        viewport: [u32; 2],
        max_width: Option<f32>,
        atlas: &mut GlyphAtlas,
    ) -> Self {
        let mut vertices = Vec::new();
        if !style.valid()
            || viewport.contains(&0)
            || !anchor.iter().all(|v| v.is_finite())
            || max_width.is_some_and(|width| !width.is_finite() || width <= 0.0)
        {
            return Self {
                vertices,
                metrics: TextMetrics::default(),
            };
        }
        let scale = style.font_size / RASTER_SIZE;
        let font = atlas.font.as_scaled(RASTER_SIZE);
        let ascent = font.ascent() * scale;
        let line_height = (font.height() + font.line_gap()) * scale;
        let mut remaining = MAX_GLYPHS;
        let mut lines = Vec::new();
        for authored_line in content.0.split('\n').take(MAX_GLYPHS) {
            let wrapped = wrap_line(authored_line, scale, max_width, atlas, &mut remaining);
            lines.extend(wrapped);
            if remaining == 0 || lines.len() >= MAX_GLYPHS {
                break;
            }
        }
        lines.truncate(MAX_GLYPHS);
        let mut measured_width = 0.0f32;
        for (line_number, glyphs) in lines.iter().enumerate() {
            let width: f32 = glyphs.iter().map(|g| g.advance * scale).sum();
            measured_width = measured_width.max(width);
            let shift = match style.alignment {
                Alignment::Left => 0.0,
                Alignment::Center => width * 0.5,
                Alignment::Right => width,
            };
            let mut pen = anchor[0] - shift;
            let baseline = anchor[1] + ascent + line_number as f32 * line_height;
            for glyph in glyphs.iter().copied() {
                let [bx, by, w, h] = glyph.bounds;
                let x = pen + bx * scale;
                let y = baseline + by * scale;
                pen += glyph.advance * scale;
                if w == 0.0 || h == 0.0 {
                    continue;
                }
                let u = (glyph.cell % 16 * CELL + 1) as f32;
                let v = (glyph.cell / 16 * CELL + 1) as f32;
                let corners = [
                    (x, y, u, v),
                    (x + w * scale, y, u + w, v),
                    (x + w * scale, y + h * scale, u + w, v + h),
                    (x, y + h * scale, u, v + h),
                ];
                for i in [0, 1, 2, 0, 2, 3] {
                    let (x, y, u, v) = corners[i];
                    vertices.push(Vertex {
                        position: [
                            x / viewport[0] as f32 * 2.0 - 1.0,
                            1.0 - y / viewport[1] as f32 * 2.0,
                        ],
                        uv: [u / ATLAS_SIZE as f32, v / ATLAS_SIZE as f32],
                        color: style.color,
                    });
                }
            }
        }
        Self {
            vertices,
            metrics: TextMetrics {
                width: measured_width,
                height: line_height * lines.len() as f32,
                line_count: u32::try_from(lines.len()).unwrap_or(u32::MAX),
            },
        }
    }
}

fn wrap_line(
    line: &str,
    scale: f32,
    max_width: Option<f32>,
    atlas: &mut GlyphAtlas,
    remaining: &mut usize,
) -> Vec<Vec<CachedGlyph>> {
    if line.is_empty() {
        return vec![Vec::new()];
    }
    let Some(max_width) = max_width else {
        let glyphs: Vec<_> = line
            .chars()
            .take(*remaining)
            .map(|ch| atlas.get(ch))
            .collect();
        *remaining = (*remaining).saturating_sub(glyphs.len());
        return vec![glyphs];
    };

    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut current_width = 0.0;
    for token in line.split_inclusive(' ') {
        if *remaining == 0 {
            break;
        }
        let token_glyphs: Vec<_> = token
            .chars()
            .take(*remaining)
            .map(|ch| atlas.get(ch))
            .collect();
        *remaining = (*remaining).saturating_sub(token_glyphs.len());
        let token_width: f32 = token_glyphs.iter().map(|g| g.advance * scale).sum();
        if !current.is_empty() && current_width + token_width > max_width {
            lines.push(std::mem::take(&mut current));
            current_width = 0.0;
        }
        for glyph in token_glyphs {
            let advance = glyph.advance * scale;
            if !current.is_empty() && current_width + advance > max_width {
                lines.push(std::mem::take(&mut current));
                current_width = 0.0;
            }
            current.push(glyph);
            current_width += advance;
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

pub struct TextRenderer {
    atlas: GlyphAtlas,
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    count: u32,
    uploads: usize,
}
impl TextRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Result<Self, String> {
        let atlas = GlyphAtlas::new()?;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("text-v0-atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text-v0"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/text.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text-v0"), layout: None,
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), compilation_options: Default::default(), buffers: &[Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4] })] },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs_main"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(), multiview_mask: None, cache: None,
        });
        let view = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text-v0"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("text-v0-vertices"),
            size: (MAX_GLYPHS * 6 * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            atlas,
            texture,
            bind_group,
            pipeline,
            buffer,
            count: 0,
            uploads: 0,
        })
    }
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        content: &TextContent,
        style: TextStyle,
        anchor: [f32; 2],
        viewport: [u32; 2],
    ) -> TextMetrics {
        let layout = TextLayout::new(content, style, anchor, viewport, &mut self.atlas);
        self.upload_and_store(queue, layout)
    }

    pub(crate) fn prepare_block(
        &mut self,
        queue: &wgpu::Queue,
        block: &TextBlock,
        viewport: [u32; 2],
    ) -> TextMetrics {
        let layout = TextLayout::with_max_width(
            &block.content,
            block.style,
            block.anchor,
            viewport,
            block.max_width,
            &mut self.atlas,
        );
        self.upload_and_store(queue, layout)
    }

    pub(crate) fn clear(&mut self) {
        self.count = 0;
    }

    fn upload_and_store(&mut self, queue: &wgpu::Queue, layout: TextLayout) -> TextMetrics {
        for (cell, pixels) in self.atlas.pending.drain(..) {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: cell % 16 * CELL,
                        y: cell / 16 * CELL,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(CELL),
                    rows_per_image: Some(CELL),
                },
                wgpu::Extent3d {
                    width: CELL,
                    height: CELL,
                    depth_or_array_layers: 1,
                },
            );
            self.uploads += 1;
        }
        self.count = layout.vertices.len() as u32;
        if self.count > 0 {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&layout.vertices));
        }
        layout.metrics
    }
    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        if self.count == 0 {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("text-v0-ui"),
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
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.draw(0..self.count, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn layout(text: &str, alignment: Alignment) -> TextLayout {
        TextLayout::new(
            &TextContent(text.into()),
            TextStyle {
                alignment,
                ..Default::default()
            },
            [200.0, 20.0],
            [800, 600],
            &mut GlyphAtlas::new().unwrap(),
        )
    }
    #[test]
    fn style_defaults_and_invalid_input() {
        let style = TextStyle::default();
        assert_eq!(style.font_size, 24.0);
        assert_eq!(style.color, [1.0; 4]);
        assert_eq!(style.alignment, Alignment::Left);
        for size in [0.0, -1.0, f32::NAN, f32::INFINITY, 129.0] {
            assert!(
                !TextStyle {
                    font_size: size,
                    ..style
                }
                .valid()
            );
        }
        assert!(
            !TextStyle {
                color: [2.0; 4],
                ..style
            }
            .valid()
        );
        assert!(
            TextStyle {
                font_size: 128.0,
                ..style
            }
            .valid()
        );
    }
    #[test]
    fn alignment_offsets_each_line_independently() {
        let left = layout("AA\nA", Alignment::Left);
        let center = layout("AA\nA", Alignment::Center);
        let right = layout("AA\nA", Alignment::Right);
        let advance = GlyphAtlas::new().unwrap().get('A').advance * 0.5;
        for (index, width) in [(0, advance * 2.0), (12, advance)] {
            let x = left.vertices[index].position[0];
            assert!((center.vertices[index].position[0] - (x - width / 800.0)).abs() < 1e-6);
            assert!((right.vertices[index].position[0] - (x - width * 2.0 / 800.0)).abs() < 1e-6);
        }
    }
    #[test]
    fn only_explicit_newlines_break_lines() {
        let a = layout("A\nA", Alignment::Left);
        let b = layout("A\n\nA\n", Alignment::Left);
        assert_eq!(a.vertices.len(), 12);
        assert_eq!(b.vertices.len(), 12);
        let first_y = a.vertices[0].position[1];
        assert!(
            (first_y - b.vertices[6].position[1] - 2.0 * (first_y - a.vertices[6].position[1]))
                .abs()
                < 1e-6
        );
        let long = layout(&"A".repeat(100), Alignment::Left);
        assert_eq!(long.vertices.len(), 600);
        assert_eq!(long.vertices[0].position[1], long.vertices[594].position[1]);
        assert!(layout("\n\n", Alignment::Left).vertices.is_empty());
        assert!(layout("", Alignment::Left).vertices.is_empty());
    }
    #[test]
    fn max_width_wraps_in_the_generic_text_layout_and_reports_metrics() {
        let mut atlas = GlyphAtlas::new().unwrap();
        let a_width = atlas.get('A').advance * 0.5;
        let max_width = a_width * 4.1;
        let wrapped = TextLayout::with_max_width(
            &TextContent("AAAA AAAA".into()),
            TextStyle::default(),
            [20.0, 20.0],
            [800, 600],
            Some(max_width),
            &mut atlas,
        );
        assert!(wrapped.metrics.line_count >= 2);
        assert!(wrapped.metrics.width <= max_width + 1e-4);
        assert!(wrapped.metrics.height > TextStyle::default().font_size);
        assert_eq!(wrapped.vertices.len(), 8 * 6);
    }
    #[test]
    fn explicit_empty_lines_are_included_in_text_metrics() {
        let laid_out = layout("A\n\nA\n", Alignment::Left);
        assert_eq!(laid_out.metrics.line_count, 4);
        assert!(laid_out.metrics.height > TextStyle::default().font_size * 3.0);
    }
    #[test]
    fn missing_glyph_uses_same_font_placeholder_and_cache() {
        let mut atlas = GlyphAtlas::new().unwrap();
        let placeholder = atlas.get('?');
        for ch in ['א', '😀', '\0', '\r', '\t', '\u{10ffff}'] {
            assert_eq!(atlas.get(ch), placeholder);
        }
        assert_eq!(atlas.glyphs.len(), 1);
        assert_eq!(atlas.pending.len(), 1);
        assert_eq!(
            layout("😀", Alignment::Left).vertices,
            layout("?", Alignment::Left).vertices
        );
    }
    #[test]
    fn atlas_caches_glyphs_and_all_supported_cells_fit() {
        let mut atlas = GlyphAtlas::new().unwrap();
        let first = atlas.get('A');
        atlas.pending.clear();
        assert_eq!(atlas.get('A'), first);
        assert!(atlas.pending.is_empty());
        for ch in (' '..='~').chain(['—']) {
            assert_ne!(atlas.font.glyph_id(ch).0, 0);
            let g = atlas.get(ch);
            assert!(g.cell < (ATLAS_SIZE / CELL).pow(2));
            if ch != ' ' {
                assert!(g.bounds[2] > 0.0 && g.bounds[3] > 0.0);
            }
        }
        assert_eq!(atlas.glyphs.len(), 96);
    }
    #[test]
    fn layout_applies_size_color_and_bounds_work() {
        let mut atlas = GlyphAtlas::new().unwrap();
        let style = TextStyle {
            font_size: 48.0,
            color: [0.2, 0.3, 0.4, 0.5],
            alignment: Alignment::Right,
        };
        let l = TextLayout::new(
            &TextContent("A".repeat(MAX_GLYPHS + 1)),
            style,
            [20.0, 20.0],
            [800, 600],
            &mut atlas,
        );
        assert_eq!(l.vertices.len(), MAX_GLYPHS * 6);
        assert_eq!(l.vertices[0].color, style.color);
        let small = layout("A", Alignment::Left);
        let large_width = l.vertices[1].position[0] - l.vertices[0].position[0];
        let small_width = small.vertices[1].position[0] - small.vertices[0].position[0];
        assert!((large_width - small_width * 2.0).abs() < 1e-4);
    }
    #[test]
    fn repeated_preparation_reuses_atlas_across_style_and_viewport_changes() {
        let mut atlas = GlyphAtlas::new().unwrap();
        let content = TextContent("PURGATORY — Text v0\nNative UI text online".into());
        let first = TextLayout::new(
            &content,
            TextStyle::default(),
            [100.0, 24.0],
            [800, 600],
            &mut atlas,
        );
        assert!(!first.vertices.is_empty());
        let count = atlas.glyphs.len();
        assert!(count > 0);
        atlas.pending.clear(); // The renderer drains these into GPU uploads.
        for alignment in [Alignment::Left, Alignment::Center, Alignment::Right] {
            let next = TextLayout::new(
                &content,
                TextStyle {
                    font_size: 32.0,
                    color: [0.5; 4],
                    alignment,
                },
                [500.0, 40.0],
                [1000, 800],
                &mut atlas,
            );
            assert_eq!(next.vertices.len(), first.vertices.len());
            assert_eq!(atlas.glyphs.len(), count);
            assert!(atlas.pending.is_empty());
        }
    }
    #[test]
    #[ignore = "requires a GPU adapter; run explicitly for Text v0 verification"]
    fn renderer_repeated_text_reuses_gpu_glyph_resources() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("GPU adapter for text renderer test");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let mut renderer = TextRenderer::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb).unwrap();
        let content = TextContent("PURGATORY — Text v0\nNative UI text online".into());
        renderer.prepare(
            &queue,
            &content,
            TextStyle::default(),
            [400.0, 24.0],
            [800, 600],
        );
        let uploads = renderer.uploads;
        assert!(uploads > 0);
        renderer.prepare(
            &queue,
            &content,
            TextStyle {
                font_size: 32.0,
                alignment: Alignment::Right,
                ..Default::default()
            },
            [500.0, 24.0],
            [1000, 800],
        );
        assert_eq!(renderer.uploads, uploads);
        assert_eq!(renderer.atlas.glyphs.len(), uploads);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 1000,
                height: 800,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer.draw(&mut encoder, &target.create_view(&Default::default()));
        queue.submit([encoder.finish()]);
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    }
}
