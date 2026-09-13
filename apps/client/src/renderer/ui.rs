//! Generic framebuffer-pixel UI primitives.
//!
//! This layer knows nothing about dialogue, text, or NPCs. It provides a small
//! screen-space colored/textured rectangle budget separate from the world
//! `MAX_QUADS` budget.

use bytemuck::{Pod, Zeroable};

use super::gpu::SpriteTextureId;

const MAX_UI_RECTS: usize = 64;
// Window chrome + a full 5x7 slot grid + one item icon per slot remain within
// this fixed submission budget.
const MAX_UI_TEXTURED_RECTS: usize = 128;

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

/// Generic screen-space textured rectangle. Coordinates use framebuffer pixels
/// with a top-left origin; UVs use the same top-left-to-bottom-right convention.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UiTexturedRect {
    pub min: [f32; 2],
    pub max: [f32; 2],
    pub texture: SpriteTextureId,
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub tint: [f32; 4],
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UiTextureRun {
    texture: SpriteTextureId,
    first_vertex: u32,
    vertex_count: u32,
}

pub(crate) struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    count: u32,
    textured_pipeline: wgpu::RenderPipeline,
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
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screen-ui-rect-vertices"),
            size: (MAX_UI_RECTS * 6 * std::mem::size_of::<UiVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
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
                        2 => Float32x4
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
        let textured_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screen-ui-texture-vertices"),
            size: (MAX_UI_TEXTURED_RECTS * 6 * std::mem::size_of::<UiTexturedVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            buffer,
            count: 0,
            textured_pipeline,
            textured_buffer,
            textured_runs: Vec::new(),
        }
    }

    pub(crate) fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        rects: &[UiRect],
        textured_rects: &[UiTexturedRect],
        viewport: [u32; 2],
    ) {
        self.count = 0;
        self.textured_runs.clear();
        if viewport.contains(&0) {
            return;
        }
        let mut vertices = Vec::with_capacity(rects.len().min(MAX_UI_RECTS) * 6);
        for rect in rects.iter().take(MAX_UI_RECTS) {
            if !rect
                .min
                .iter()
                .chain(rect.max.iter())
                .all(|v| v.is_finite())
                || rect.max[0] <= rect.min[0]
                || rect.max[1] <= rect.min[1]
                || !rect
                    .color
                    .iter()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            {
                continue;
            }
            let to_ndc = |point: [f32; 2]| {
                [
                    point[0] / viewport[0] as f32 * 2.0 - 1.0,
                    1.0 - point[1] / viewport[1] as f32 * 2.0,
                ]
            };
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
        self.count = u32::try_from(vertices.len()).unwrap_or(0);
        if self.count > 0 {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&vertices));
        }

        let mut textured_vertices =
            Vec::with_capacity(textured_rects.len().min(MAX_UI_TEXTURED_RECTS) * 6);
        for rect in textured_rects.iter().take(MAX_UI_TEXTURED_RECTS) {
            if !valid_textured_rect(rect) {
                continue;
            }
            let first_vertex = u32::try_from(textured_vertices.len()).unwrap_or(0);
            let to_ndc = |point: [f32; 2]| {
                [
                    point[0] / viewport[0] as f32 * 2.0 - 1.0,
                    1.0 - point[1] / viewport[1] as f32 * 2.0,
                ]
            };
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
            for index in [0, 1, 2, 0, 2, 3] {
                textured_vertices.push(UiTexturedVertex {
                    position: positions[index],
                    uv: uvs[index],
                    tint: rect.tint,
                });
            }
            if let Some(run) = self.textured_runs.last_mut()
                && run.texture == rect.texture
            {
                run.vertex_count += 6;
            } else {
                self.textured_runs.push(UiTextureRun {
                    texture: rect.texture,
                    first_vertex,
                    vertex_count: 6,
                });
            }
        }
        if !textured_vertices.is_empty() {
            queue.write_buffer(
                &self.textured_buffer,
                0,
                bytemuck::cast_slice(&textured_vertices),
            );
        }
    }

    pub(crate) fn draw_textured<'a>(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        mut texture_bind_group: impl FnMut(SpriteTextureId) -> Option<&'a wgpu::BindGroup>,
    ) {
        if self.textured_runs.is_empty() {
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
        pass.set_vertex_buffer(0, self.textured_buffer.slice(..));
        for run in &self.textured_runs {
            let Some(bind_group) = texture_bind_group(run.texture) else {
                continue;
            };
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(run.first_vertex..run.first_vertex + run.vertex_count, 0..1);
        }
    }

    pub(crate) fn draw(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        if self.count == 0 {
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
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.draw(0..self.count, 0..1);
    }
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

    fn texture_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("test-ui-texture-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        })
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
    #[ignore = "requires a GPU adapter; run explicitly for screen UI verification"]
    fn renderer_submits_screen_rect_pass() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("GPU adapter for screen UI test");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let layout = texture_layout(&device);
        let mut renderer = UiRenderer::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb, &layout);
        renderer.prepare(
            &queue,
            &[UiRect {
                min: [10.0, 20.0],
                max: [200.0, 100.0],
                color: [0.2, 0.3, 0.4, 0.8],
            }],
            &[],
            [800, 600],
        );
        assert_eq!(renderer.count, 6);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 800,
                height: 600,
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

    #[test]
    #[ignore = "requires a GPU adapter; run explicitly for textured screen UI verification"]
    fn renderer_submits_textured_screen_rect_pass() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("GPU adapter for textured screen UI test");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let layout = texture_layout(&device);
        let mut renderer = UiRenderer::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb, &layout);
        let texture_id = SpriteTextureId::from_raw(7);
        renderer.prepare(
            &queue,
            &[],
            &[UiTexturedRect {
                min: [10.0, 20.0],
                max: [200.0, 100.0],
                texture: texture_id,
                uv_min: [0.0, 0.0],
                uv_max: [1.0, 1.0],
                tint: [1.0; 4],
            }],
            [800, 600],
        );
        assert_eq!(renderer.textured_runs.len(), 1);
        assert_eq!(renderer.textured_runs[0].vertex_count, 6);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("test-ui-source"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255, 255, 255, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("test-ui-source"),
            layout: &layout,
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
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("test-ui-target"),
            size: wgpu::Extent3d {
                width: 800,
                height: 600,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let target_view = target.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer.draw_textured(&mut encoder, &target_view, |id| {
            (id == texture_id).then_some(&bind_group)
        });
        queue.submit([encoder.finish()]);
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    }
}
