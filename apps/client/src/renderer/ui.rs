//! Generic framebuffer-pixel UI rectangles.
//!
//! This layer knows nothing about dialogue, text, or NPCs. It provides a small
//! screen-space shape budget separate from the world `MAX_QUADS` budget.

use bytemuck::{Pod, Zeroable};

const MAX_UI_RECTS: usize = 64;

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

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct UiVertex {
    position: [f32; 2],
    color: [f32; 4],
}

pub(crate) struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    count: u32,
}

impl UiRenderer {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
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
        Self {
            pipeline,
            buffer,
            count: 0,
        }
    }

    pub(crate) fn prepare(&mut self, queue: &wgpu::Queue, rects: &[UiRect], viewport: [u32; 2]) {
        self.count = 0;
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut renderer = UiRenderer::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
        renderer.prepare(
            &queue,
            &[UiRect {
                min: [10.0, 20.0],
                max: [200.0, 100.0],
                color: [0.2, 0.3, 0.4, 0.8],
            }],
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
}
