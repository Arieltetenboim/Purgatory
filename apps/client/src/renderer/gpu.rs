//! Phase-2 primitive renderer built directly on wgpu.

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use winit::window::Window;

use super::camera::{Camera, FOOTNOTE_LOGICAL_HEIGHT, is_usable_surface};

const SHADER: &str = include_str!("shaders/primitive.wgsl");
const MAX_QUADS: usize = 192;

/// Colored rectangle in world units. Presentation only.
#[derive(Clone, Copy, Debug)]
pub struct DrawQuad {
    pub center: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

/// How a frame attempt should be handled by the application loop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameStatus {
    Drawn,
    Skipped,
    NeedsReconfigure,
    DeviceLost,
}

/// GPU handles for a post-world overlay pass. No egui types.
pub struct OverlayPass<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub view: &'a wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

/// Client-only wgpu renderer. Not a reusable engine layer.
pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    camera: Camera,
    adapter_name: String,
    backend: wgpu::Backend,
    frames_drawn: u64,
}

impl Renderer {
    pub fn new(window: Arc<Window>) -> Result<Self, String> {
        let mut instance_desc = wgpu::InstanceDescriptor::new_without_display_handle();
        instance_desc.backends = wgpu::Backends::PRIMARY;
        let instance = wgpu::Instance::new(instance_desc);

        let surface = instance
            .create_surface(window.clone())
            .map_err(|err| format!("create surface: {err}"))?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|err| format!("request adapter: {err}"))?;

        let info = adapter.get_info();
        let device_desc = wgpu::DeviceDescriptor {
            label: Some("purgatory-client-device"),
            ..Default::default()
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&device_desc))
            .map_err(|err| format!("request device: {err}"))?;

        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or_else(|| "adapter does not support the window surface".to_string())?;
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("purgatory-primitive-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("purgatory-camera-bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("purgatory-primitive-layout"),
            bind_group_layouts: &[Some(&camera_bind_group_layout)],
            immediate_size: 0,
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 8,
                    shader_location: 1,
                },
            ],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("purgatory-primitive-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(vertex_layout)],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("purgatory-vertices"),
            size: (MAX_QUADS * 4 * std::mem::size_of::<Vertex>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut indices = Vec::with_capacity(MAX_QUADS * 6);
        for quad in 0..MAX_QUADS {
            let base = (quad * 4) as u16;
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("purgatory-indices"),
            size: std::mem::size_of_val(indices.as_slice()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&index_buffer, 0, bytemuck::cast_slice(&indices));

        let camera = Camera::from_physical_pixels_with_height(
            width,
            height,
            FOOTNOTE_LOGICAL_HEIGHT,
            [0.0, 0.0],
        )
        .unwrap_or_else(Camera::footnote_test_dev);
        let camera_uniform = CameraUniform::from_camera(&camera);
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("purgatory-camera"),
            size: std::mem::size_of::<CameraUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&camera_buffer, 0, bytemuck::bytes_of(&camera_uniform));

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("purgatory-camera-bg"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        println!(
            "PURGATORY renderer initialized adapter='{}' backend={:?} device_type={:?} format={:?} size={}x{}",
            info.name, info.backend, info.device_type, config.format, width, height
        );

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            vertex_buffer,
            index_buffer,
            index_count: 0,
            camera_buffer,
            camera_bind_group,
            camera,
            adapter_name: info.name,
            backend: info.backend,
            frames_drawn: 0,
        })
    }

    #[must_use]
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    #[must_use]
    pub fn backend(&self) -> wgpu::Backend {
        self.backend
    }

    #[must_use]
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    #[must_use]
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    #[must_use]
    pub fn max_texture_dimension_2d(&self) -> u32 {
        self.device.limits().max_texture_dimension_2d
    }

    #[must_use]
    pub fn surface_size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    #[must_use]
    pub fn frames_drawn(&self) -> u64 {
        self.frames_drawn
    }

    #[must_use]
    pub fn camera(&self) -> Camera {
        self.camera
    }

    pub fn set_camera(&mut self, camera: Camera) {
        self.camera = camera;
        self.write_camera_uniform();
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if !is_usable_surface(width, height) {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        if self.camera.set_physical_pixels(width, height) {
            self.write_camera_uniform();
        }
    }

    pub fn reconfigure(&mut self) {
        if is_usable_surface(self.config.width, self.config.height) {
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn render(
        &mut self,
        quads: &[DrawQuad],
        overlay: impl FnOnce(OverlayPass<'_>) -> Vec<wgpu::CommandBuffer>,
    ) -> FrameStatus {
        if !is_usable_surface(self.config.width, self.config.height) {
            return FrameStatus::Skipped;
        }
        self.upload_quads(quads);

        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                let status = self.draw_surface_texture(texture, overlay);
                return match status {
                    FrameStatus::Drawn => FrameStatus::NeedsReconfigure,
                    other => other,
                };
            }
            wgpu::CurrentSurfaceTexture::Timeout => {
                eprintln!("PURGATORY renderer: surface timeout; skipping frame");
                return FrameStatus::Skipped;
            }
            wgpu::CurrentSurfaceTexture::Occluded => return FrameStatus::Skipped,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                return FrameStatus::NeedsReconfigure;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                eprintln!("PURGATORY renderer: surface validation failure");
                return FrameStatus::DeviceLost;
            }
        };

        self.draw_surface_texture(surface_texture, overlay)
    }

    fn draw_surface_texture(
        &mut self,
        surface_texture: wgpu::SurfaceTexture,
        overlay: impl FnOnce(OverlayPass<'_>) -> Vec<wgpu::CommandBuffer>,
    ) -> FrameStatus {
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("purgatory-frame"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("purgatory-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.047,
                            g: 0.063,
                            b: 0.125,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            if self.index_count > 0 {
                pass.draw_indexed(0..self.index_count, 0, 0..1);
            }
        }

        let extra = overlay(OverlayPass {
            device: &self.device,
            queue: &self.queue,
            encoder: &mut encoder,
            view: &view,
            width: self.config.width,
            height: self.config.height,
        });
        self.queue
            .submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
        self.queue.present(surface_texture);
        self.frames_drawn = self.frames_drawn.saturating_add(1);
        FrameStatus::Drawn
    }

    fn write_camera_uniform(&self) {
        let uniform = CameraUniform::from_camera(&self.camera);
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    fn upload_quads(&mut self, quads: &[DrawQuad]) {
        let count = quads.len().min(MAX_QUADS);
        let mut vertices = Vec::with_capacity(count * 4);
        for quad in quads.iter().take(count) {
            vertices.extend_from_slice(&quad_vertices(*quad));
        }
        if !vertices.is_empty() {
            self.queue
                .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        }
        self.index_count = (count * 6) as u32;
    }
}

impl CameraUniform {
    fn from_camera(camera: &Camera) -> Self {
        let m = camera.view_proj_column_major();
        Self {
            view_proj: [
                [m[0], m[1], m[2], m[3]],
                [m[4], m[5], m[6], m[7]],
                [m[8], m[9], m[10], m[11]],
                [m[12], m[13], m[14], m[15]],
            ],
        }
    }
}

fn quad_vertices(quad: DrawQuad) -> [Vertex; 4] {
    let hx = quad.size[0] * 0.5;
    let hy = quad.size[1] * 0.5;
    let x = quad.center[0];
    let y = quad.center[1];
    let color = quad.color;
    [
        Vertex {
            position: [x - hx, y - hy],
            color,
        },
        Vertex {
            position: [x + hx, y - hy],
            color,
        },
        Vertex {
            position: [x + hx, y + hy],
            color,
        },
        Vertex {
            position: [x - hx, y + hy],
            color,
        },
    ]
}
