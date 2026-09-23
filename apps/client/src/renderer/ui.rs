//! Generic framebuffer-pixel UI primitives.
//!
//! This layer knows nothing about dialogue, text, or NPCs. It provides a small
//! screen-space colored/textured rectangle budget separate from the world
//! `MAX_QUADS` budget.

use bytemuck::{Pod, Zeroable};

use super::gpu::SpriteTextureId;

const MAX_UI_RECTS: usize = 64;
const MAX_UI_TEXTURED_RECTS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct UiBudgetUsage {
    pub(crate) requested_rects: usize,
    pub(crate) accepted_rects: usize,
    pub(crate) requested_textured_rects: usize,
    pub(crate) accepted_textured_rects: usize,
}

impl UiBudgetUsage {
    #[must_use]
    fn from_counts(requested_rects: usize, requested_textured_rects: usize) -> Self {
        Self {
            requested_rects,
            accepted_rects: requested_rects.min(MAX_UI_RECTS),
            requested_textured_rects,
            accepted_textured_rects: requested_textured_rects.min(MAX_UI_TEXTURED_RECTS),
        }
    }

    #[must_use]
    fn overflowed(self) -> bool {
        self.requested_rects > self.accepted_rects
            || self.requested_textured_rects > self.accepted_textured_rects
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UiRect {
    pub min: [f32; 2],
    pub max: [f32; 2],
    pub color: [f32; 4],
}

impl UiRect {
    #[must_use]
    pub(crate) fn contains(self, point: [f32; 2]) -> bool {
        point[0] >= self.min[0]
            && point[0] <= self.max[0]
            && point[1] >= self.min[1]
            && point[1] <= self.max[1]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UiTexturedRect {
    pub min: [f32; 2],
    pub max: [f32; 2],
    pub texture: SpriteTextureId,
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub tint: [f32; 4],
}

/// One atomic screen-space UI submission. Groups are rendered in slice order.
pub(crate) struct UiComposition<'a> {
    pub(crate) textured_rects: &'a [UiTexturedRect],
    pub(crate) rects: &'a [UiRect],
    pub(crate) text: &'a [super::text::TextBlock],
}

impl<'a> UiComposition<'a> {
    pub(crate) const fn new(
        textured_rects: &'a [UiTexturedRect],
        rects: &'a [UiRect],
        text: &'a [super::text::TextBlock],
    ) -> Self {
        Self {
            textured_rects,
            rects,
            text,
        }
    }
}

impl UiTexturedRect {
    #[cfg(test)]
    #[must_use]
    pub(crate) fn size(self) -> [f32; 2] {
        [self.max[0] - self.min[0], self.max[1] - self.min[1]]
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct UiVertex {
    position: [f32; 2],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct UiTexturedVertex {
    position: [f32; 2],
    uv: [f32; 2],
    tint: [f32; 4],
    uv_min: [f32; 2],
    uv_max: [f32; 2],
    rect_min: [f32; 2],
    rect_size: [f32; 2],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UiTextureRun {
    texture: SpriteTextureId,
    first_vertex: u32,
    vertex_count: u32,
}

pub(crate) struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    textured_pipeline: wgpu::RenderPipeline,
    batches: Vec<UiBatch>,
    overflow_active: bool,
}

struct UiBatch {
    buffer: wgpu::Buffer,
    count: u32,
    textured_buffer: wgpu::Buffer,
    textured_runs: Vec<UiTextureRun>,
}

impl UiRenderer {
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        texture_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("screen-ui-rect"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/ui_rect.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("screen-ui-rect"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<UiVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let textured_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("screen-ui-texture"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/ui_texture.wgsl").into()),
        });
        let textured_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("screen-ui-texture-layout"),
            bind_group_layouts: &[Some(texture_layout)],
            immediate_size: 0,
        });
        let textured_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("screen-ui-texture"),
            layout: Some(&textured_layout),
            vertex: wgpu::VertexState {
                module: &textured_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<UiTexturedVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2,
                        1 => Float32x2,
                        2 => Float32x4,
                        3 => Float32x2,
                        4 => Float32x2,
                        5 => Float32x2,
                        6 => Float32x2
                    ],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &textured_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            textured_pipeline,
            batches: Vec::new(),
            overflow_active: false,
        }
    }

    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rects: &[UiRect],
        textured_rects: &[UiTexturedRect],
        viewport: [u32; 2],
    ) -> usize {
        let mut batch = UiBatch {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("screen-ui-rect-batch"),
                size: (MAX_UI_RECTS * 6 * std::mem::size_of::<UiVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            count: 0,
            textured_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("screen-ui-texture-batch"),
                size: (MAX_UI_TEXTURED_RECTS * 6 * std::mem::size_of::<UiTexturedVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            textured_runs: Vec::new(),
        };
        if viewport.contains(&0) {
            self.batches.push(batch);
            return self.batches.len() - 1;
        }

        let budget_usage = UiBudgetUsage::from_counts(rects.len(), textured_rects.len());
        if budget_usage.overflowed() {
            if !self.overflow_active {
                eprintln!(
                    "PURGATORY UI: primitive budget overflow; rects {}/{} textured {}/{}",
                    budget_usage.requested_rects,
                    budget_usage.accepted_rects,
                    budget_usage.requested_textured_rects,
                    budget_usage.accepted_textured_rects
                );
            }
            self.overflow_active = true;
        } else {
            self.overflow_active = false;
        }
        let rects = budgeted_slice(rects, MAX_UI_RECTS);
        let textured_rects = budgeted_slice(textured_rects, MAX_UI_TEXTURED_RECTS);

        let to_ndc = |point: [f32; 2]| {
            [
                point[0] / viewport[0] as f32 * 2.0 - 1.0,
                1.0 - point[1] / viewport[1] as f32 * 2.0,
            ]
        };

        let mut vertices = Vec::with_capacity(rects.len() * 6);
        for rect in rects {
            if !valid_rect(rect) {
                continue;
            }
            let corners = [
                to_ndc(rect.min),
                to_ndc([rect.max[0], rect.min[1]]),
                to_ndc(rect.max),
                to_ndc([rect.min[0], rect.max[1]]),
            ];
            for index in [0, 1, 2, 0, 2, 3] {
                vertices.push(UiVertex {
                    position: corners[index],
                    color: rect.color,
                });
            }
        }
        batch.count = u32::try_from(vertices.len()).unwrap_or(0);
        if batch.count > 0 {
            queue.write_buffer(&batch.buffer, 0, bytemuck::cast_slice(&vertices));
        }

        let mut textured_vertices = Vec::with_capacity(textured_rects.len() * 6);
        for rect in textured_rects {
            if !valid_textured_rect(rect) {
                continue;
            }
            let first_vertex = u32::try_from(textured_vertices.len()).unwrap_or(0);
            let positions = [
                to_ndc(rect.min),
                to_ndc([rect.max[0], rect.min[1]]),
                to_ndc(rect.max),
                to_ndc([rect.min[0], rect.max[1]]),
            ];
            let uvs = [
                rect.uv_min,
                [rect.uv_max[0], rect.uv_min[1]],
                rect.uv_max,
                [rect.uv_min[0], rect.uv_max[1]],
            ];
            let rect_size = [rect.max[0] - rect.min[0], rect.max[1] - rect.min[1]];
            for index in [0, 1, 2, 0, 2, 3] {
                textured_vertices.push(UiTexturedVertex {
                    position: positions[index],
                    uv: uvs[index],
                    tint: rect.tint,
                    uv_min: rect.uv_min,
                    uv_max: rect.uv_max,
                    rect_min: rect.min,
                    rect_size,
                });
            }
            if let Some(run) = batch.textured_runs.last_mut()
                && run.texture == rect.texture
            {
                run.vertex_count += 6;
            } else {
                batch.textured_runs.push(UiTextureRun {
                    texture: rect.texture,
                    first_vertex,
                    vertex_count: 6,
                });
            }
        }
        if !textured_vertices.is_empty() {
            queue.write_buffer(
                &batch.textured_buffer,
                0,
                bytemuck::cast_slice(&textured_vertices),
            );
        }
        self.batches.push(batch);
        self.batches.len() - 1
    }

    pub(crate) fn draw_textured<'a>(
        &self,
        batch_index: usize,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        mut texture_bind_group: impl FnMut(SpriteTextureId) -> Option<&'a wgpu::BindGroup>,
    ) {
        let Some(batch) = self.batches.get(batch_index) else {
            return;
        };
        if batch.textured_runs.is_empty() {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("screen-ui-texture"),
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
        pass.set_pipeline(&self.textured_pipeline);
        pass.set_vertex_buffer(0, batch.textured_buffer.slice(..));
        for run in &batch.textured_runs {
            let Some(bind_group) = texture_bind_group(run.texture) else {
                continue;
            };
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(run.first_vertex..run.first_vertex + run.vertex_count, 0..1);
        }
    }

    pub(crate) fn draw(
        &self,
        batch_index: usize,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) {
        let Some(batch) = self.batches.get(batch_index) else {
            return;
        };
        if batch.count == 0 {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("screen-ui-rect"),
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
        pass.set_vertex_buffer(0, batch.buffer.slice(..));
        pass.draw(0..batch.count, 0..1);
    }

    pub(crate) fn clear(&mut self) {
        self.batches.clear();
    }
}

fn budgeted_slice<T>(items: &[T], max: usize) -> &[T] {
    &items[..items.len().min(max)]
}

fn valid_rect(rect: &UiRect) -> bool {
    rect.min
        .iter()
        .chain(rect.max.iter())
        .all(|v| v.is_finite())
        && rect.max[0] > rect.min[0]
        && rect.max[1] > rect.min[1]
        && rect
            .color
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
}

fn valid_textured_rect(rect: &UiTexturedRect) -> bool {
    rect.min
        .iter()
        .chain(rect.max.iter())
        .chain(rect.uv_min.iter())
        .chain(rect.uv_max.iter())
        .all(|value| value.is_finite())
        && rect.max[0] > rect.min[0]
        && rect.max[1] > rect.min[1]
        && rect.uv_max[0] > rect.uv_min[0]
        && rect.uv_max[1] > rect.uv_min[1]
        && rect
            .uv_min
            .iter()
            .chain(rect.uv_max.iter())
            .all(|value| (0.0..=1.0).contains(value))
        && rect
            .tint
            .iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_budget_usage_caps_and_reports_overflow_independently() {
        let rects = vec![(); MAX_UI_RECTS + 3];
        let textured_rects = vec![(); MAX_UI_TEXTURED_RECTS + 5];
        let usage = UiBudgetUsage::from_counts(rects.len(), textured_rects.len());

        assert_eq!(budgeted_slice(&rects, MAX_UI_RECTS).len(), MAX_UI_RECTS);
        assert_eq!(
            budgeted_slice(&textured_rects, MAX_UI_TEXTURED_RECTS).len(),
            MAX_UI_TEXTURED_RECTS
        );
        assert_eq!(usage.requested_rects, MAX_UI_RECTS + 3);
        assert_eq!(usage.accepted_rects, MAX_UI_RECTS);
        assert_eq!(usage.requested_textured_rects, MAX_UI_TEXTURED_RECTS + 5);
        assert_eq!(usage.accepted_textured_rects, MAX_UI_TEXTURED_RECTS);
        assert!(usage.overflowed());

        let at_cap = UiBudgetUsage::from_counts(MAX_UI_RECTS, MAX_UI_TEXTURED_RECTS);
        assert!(!at_cap.overflowed());
    }

    #[test]
    fn hit_bounds_include_edges() {
        let rect = UiRect {
            min: [10.0, 20.0],
            max: [30.0, 40.0],
            color: [1.0; 4],
        };
        assert!(rect.contains([10.0, 20.0]));
        assert!(rect.contains([30.0, 40.0]));
        assert!(!rect.contains([9.0, 20.0]));
    }

    #[test]
    fn textured_rect_validation_rejects_bad_uvs() {
        let texture = SpriteTextureId::from_raw(7);
        let mut rect = UiTexturedRect {
            min: [10.0, 20.0],
            max: [30.0, 40.0],
            texture,
            uv_min: [0.1, 0.1],
            uv_max: [0.9, 0.9],
            tint: [1.0; 4],
        };
        assert!(valid_textured_rect(&rect));
        rect.uv_max[0] = rect.uv_min[0];
        assert!(!valid_textured_rect(&rect));
    }

    #[test]
    fn compositions_keep_submission_order_and_primitive_collections() {
        let rects = [UiRect {
            min: [0.0, 0.0],
            max: [1.0, 1.0],
            color: [1.0; 4],
        }];
        let textured = [UiTexturedRect {
            min: [0.0, 0.0],
            max: [1.0, 1.0],
            texture: SpriteTextureId::from_raw(1),
            uv_min: [0.0, 0.0],
            uv_max: [1.0, 1.0],
            tint: [1.0; 4],
        }];
        let text = [crate::renderer::text::TextBlock {
            content: crate::renderer::text::TextContent("group".to_owned()),
            style: crate::renderer::text::TextStyle::default(),
            anchor: [0.0, 0.0],
            max_width: None,
        }];
        let first = UiComposition::new(&textured, &rects, &text);
        let second = UiComposition::new(&textured, &rects, &text);
        let ordered = [first, second];

        assert_eq!(ordered[0].textured_rects.len(), 1);
        assert_eq!(ordered[0].rects.len(), 1);
        assert_eq!(ordered[0].text.len(), 1);
        assert_eq!(ordered[1].textured_rects.len(), 1);
        assert_eq!(ordered[1].rects.len(), 1);
        assert_eq!(ordered[1].text.len(), 1);
    }
}
