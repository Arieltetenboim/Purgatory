//! Phase-2 primitive renderer built directly on wgpu.

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use winit::window::Window;

use crate::asset_runtime::SpriteResource;
use crate::display::{
    RenderScale, SurfaceResizeAction, WorldTargetAction, classify_framebuffer_resize,
    classify_world_target_resize, internal_render_size,
};

use super::camera::{
    Camera, FOOTNOTE_LOGICAL_HEIGHT, PixelViewport, constrained_pixel_viewport, is_usable_surface,
};
#[cfg(feature = "dev-diagnostics")]
use super::rf_diag::{
    RfAbPanel, RfAbSlot, rf_ab_camera, rf_ab_layout, rf_ab_panel_source, rf_ab_scene_quads,
};

const SHADER: &str = include_str!("shaders/primitive.wgsl");
const BLIT_SHADER: &str = include_str!("shaders/blit.wgsl");
/// Offscreen world target. Independent of swapchain format so it is always
/// sampleable. Blit converts to the surface format.
const WORLD_TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// Coverage samples for [`WorldMsaa::X4`]. Thin skeleton/placeholder quads stay
/// in world units; MSAA approximates pixel coverage at whatever internal
/// resolution Render Scale selected, then the resolve is linearly blitted.
const WORLD_MSAA_X4: u32 = 4;

/// World-pass coverage samples. Production default is 4× when the offscreen
/// format supports it. 1× (`Off`) is compatibility / diagnostic fallback only.
/// Does not change camera FOV or gameplay visibility.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WorldMsaa {
    Off,
    #[default]
    X4,
}

impl WorldMsaa {
    pub const ALL: [Self; 2] = [Self::Off, Self::X4];

    #[must_use]
    pub const fn sample_count(self) -> u32 {
        match self {
            Self::Off => 1,
            Self::X4 => WORLD_MSAA_X4,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::X4 => "4×",
        }
    }
}

/// True when `Rgba8UnormSrgb` can be a 4× render target that resolves.
#[must_use]
pub fn world_format_supports_4x_msaa(flags: wgpu::TextureFormatFeatureFlags) -> bool {
    flags.sample_count_supported(WORLD_MSAA_X4)
        && flags.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE)
}
const LETTERBOX_CLEAR: wgpu::Color = wgpu::Color {
    r: 0.047,
    g: 0.063,
    b: 0.125,
    a: 1.0,
};

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 4] = [
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
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 24,
        shader_location: 2,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32,
        offset: 32,
        shader_location: 3,
    },
];
const DUMMY_UVS: [[f32; 2]; 4] = [[0.0, 0.0]; 4];
/// Hard cap on world quads uploaded this frame. Excess is truncated.
pub const MAX_QUADS: usize = 192;

/// Renderer/client identity for a texture used by a world sprite.
///
/// This is deliberately separate from content, visual, filesystem, and
/// gameplay identities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpriteTextureId(u32);

impl SpriteTextureId {
    #[cfg(test)]
    pub const HEADWEAR: Self = Self(1);

    pub(crate) const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub(crate) const fn raw(self) -> u32 {
        self.0
    }
}

/// Colored rectangle (or triangle) in world units. Presentation only.
///
/// Axis-aligned callers use [`DrawQuad::rect`] / [`DrawQuad::triangle`].
/// Oriented cutouts use [`DrawQuad::oriented`]; convex four-corner solids use
/// [`DrawQuad::convex`]. Textured sprites use [`DrawQuad::textured_sprite`].
/// The GPU still uploads the same world-space vertices. This type does not
/// know about bones or slots.
#[derive(Clone, Copy, Debug)]
pub struct DrawQuad {
    pub center: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    /// When true, vertices form an upward triangle instead of an AABB.
    pub triangle: bool,
    uvs: [[f32; 2]; 4],
    sprite_texture: Option<SpriteTextureId>,
    layout: QuadLayout,
}

#[derive(Clone, Copy, Debug)]
enum QuadLayout {
    AxisAligned,
    Oriented {
        pivot: [f32; 2],
        local_center: [f32; 2],
        rotation: f32,
    },
    Convex {
        pivot: [f32; 2],
        rotation: f32,
        local_corners: [[f32; 2]; 4],
    },
}

impl DrawQuad {
    #[must_use]
    pub const fn rect(center: [f32; 2], size: [f32; 2], color: [f32; 4]) -> Self {
        Self {
            center,
            size,
            color,
            triangle: false,
            uvs: DUMMY_UVS,
            sprite_texture: None,
            layout: QuadLayout::AxisAligned,
        }
    }

    #[must_use]
    pub const fn triangle(center: [f32; 2], size: [f32; 2], color: [f32; 4]) -> Self {
        Self {
            center,
            size,
            color,
            triangle: true,
            uvs: DUMMY_UVS,
            sprite_texture: None,
            layout: QuadLayout::AxisAligned,
        }
    }

    /// Solid rectangle. Corners: `world = pivot + R(rotation) × local_corner`,
    /// where each local corner is `local_center ± half size`.
    #[must_use]
    pub fn oriented(
        pivot: [f32; 2],
        size: [f32; 2],
        local_center: [f32; 2],
        rotation: f32,
        color: [f32; 4],
    ) -> Self {
        let (sin, cos) = rotation.sin_cos();
        let visual = rotate_local(local_center, sin, cos);
        Self {
            center: [pivot[0] + visual[0], pivot[1] + visual[1]],
            size,
            color,
            triangle: false,
            uvs: DUMMY_UVS,
            sprite_texture: None,
            layout: QuadLayout::Oriented {
                pivot,
                local_center,
                rotation,
            },
        }
    }

    /// Solid convex quad from four local-space corners.
    ///
    /// `world = pivot + R(rotation) × local_corner`. Winding must match
    /// [`oriented`]: bottom-left, bottom-right, top-right, top-left (CCW).
    /// Not a general polygon API; callers supply a convex four-corner solid.
    #[must_use]
    pub fn convex(
        pivot: [f32; 2],
        local_corners: [[f32; 2]; 4],
        rotation: f32,
        color: [f32; 4],
    ) -> Self {
        let (sin, cos) = rotation.sin_cos();
        let centroid = [
            (local_corners[0][0] + local_corners[1][0] + local_corners[2][0] + local_corners[3][0])
                * 0.25,
            (local_corners[0][1] + local_corners[1][1] + local_corners[2][1] + local_corners[3][1])
                * 0.25,
        ];
        let visual = rotate_local(centroid, sin, cos);
        let min_x = local_corners
            .iter()
            .map(|p| p[0])
            .fold(f32::INFINITY, f32::min);
        let max_x = local_corners
            .iter()
            .map(|p| p[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let min_y = local_corners
            .iter()
            .map(|p| p[1])
            .fold(f32::INFINITY, f32::min);
        let max_y = local_corners
            .iter()
            .map(|p| p[1])
            .fold(f32::NEG_INFINITY, f32::max);
        Self {
            center: [pivot[0] + visual[0], pivot[1] + visual[1]],
            size: [max_x - min_x, max_y - min_y],
            color,
            triangle: false,
            uvs: DUMMY_UVS,
            sprite_texture: None,
            layout: QuadLayout::Convex {
                pivot,
                rotation,
                local_corners,
            },
        }
    }

    /// Textured convex sprite. Same world corners as [`Self::convex`].
    /// UVs follow GPU winding (bottom-left, bottom-right, top-right, top-left).
    #[must_use]
    pub fn textured_sprite(
        sprite_texture: SpriteTextureId,
        pivot: [f32; 2],
        local_corners: [[f32; 2]; 4],
        uvs: [[f32; 2]; 4],
        rotation: f32,
    ) -> Self {
        let mut quad = Self::convex(pivot, local_corners, rotation, [1.0, 1.0, 1.0, 1.0]);
        quad.uvs = uvs;
        quad.sprite_texture = Some(sprite_texture);
        quad
    }

    /// Mirror presentation geometry around a world-space vertical origin.
    #[must_use]
    pub fn mirror_x_about(self, root: [f32; 2]) -> Self {
        match self.layout {
            QuadLayout::AxisAligned => Self {
                center: [root[0] * 2.0 - self.center[0], self.center[1]],
                ..self
            },
            QuadLayout::Oriented {
                pivot,
                local_center,
                rotation,
            } => Self::oriented(
                [root[0] * 2.0 - pivot[0], pivot[1]],
                self.size,
                [-local_center[0], local_center[1]],
                -rotation,
                self.color,
            ),
            QuadLayout::Convex {
                pivot,
                rotation,
                local_corners,
            } => {
                let mirror = |corner: [f32; 2]| [-corner[0], corner[1]];
                Self::textured_or_solid_convex(
                    [root[0] * 2.0 - pivot[0], pivot[1]],
                    [
                        mirror(local_corners[1]),
                        mirror(local_corners[0]),
                        mirror(local_corners[3]),
                        mirror(local_corners[2]),
                    ],
                    -rotation,
                    self.color,
                    self.sprite_texture,
                    [self.uvs[1], self.uvs[0], self.uvs[3], self.uvs[2]],
                )
            }
        }
    }

    fn textured_or_solid_convex(
        pivot: [f32; 2],
        local_corners: [[f32; 2]; 4],
        rotation: f32,
        color: [f32; 4],
        sprite_texture: Option<SpriteTextureId>,
        uvs: [[f32; 2]; 4],
    ) -> Self {
        let mut quad = Self::convex(pivot, local_corners, rotation, color);
        quad.sprite_texture = sprite_texture;
        quad.uvs = uvs;
        quad
    }

    #[must_use]
    pub const fn is_textured(self) -> bool {
        self.sprite_texture.is_some()
    }

    #[must_use]
    pub const fn sprite_texture_id(self) -> Option<SpriteTextureId> {
        self.sprite_texture
    }

    #[must_use]
    #[cfg_attr(not(test), allow(dead_code))]
    pub const fn uvs(self) -> [[f32; 2]; 4] {
        self.uvs
    }

    /// World-space corners in the same winding as the GPU vertex expand.
    /// Textured quads use this order for UVs as well.
    #[must_use]
    pub fn world_corners(self) -> [[f32; 2]; 4] {
        match self.layout {
            QuadLayout::AxisAligned => axis_aligned_corners(self),
            QuadLayout::Oriented {
                pivot,
                local_center,
                rotation,
            } => oriented_corners(pivot, self.size, local_center, rotation),
            QuadLayout::Convex {
                pivot,
                rotation,
                local_corners,
            } => convex_corners(pivot, local_corners, rotation),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
    uv: [f32; 2],
    textured: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DrawRun {
    texture: Option<SpriteTextureId>,
    first_quad: usize,
    quad_count: usize,
}

fn build_draw_runs(world_quads: &[DrawQuad]) -> Vec<DrawRun> {
    let mut runs: Vec<DrawRun> = Vec::new();
    let count = world_quads.len().min(MAX_QUADS);
    for (quad_index, quad) in world_quads.iter().take(count).enumerate() {
        let texture = quad.sprite_texture_id();
        if let Some(run) = runs.last_mut()
            && run.texture == texture
        {
            run.quad_count += 1;
        } else {
            runs.push(DrawRun {
                texture,
                first_quad: quad_index,
                quad_count: 1,
            });
        }
    }
    runs
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

/// Offscreen world color target + blit bind group. Recreated when the applied
/// internal size or sample count changes.
struct WorldTarget {
    _msaa_texture: Option<wgpu::Texture>,
    _color_texture: wgpu::Texture,
    msaa_view: Option<wgpu::TextureView>,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    #[cfg_attr(not(feature = "dev-diagnostics"), allow(dead_code))]
    bind_group_nearest: wgpu::BindGroup,
    size: (u32, u32),
    sample_count: u32,
}

#[derive(Clone, Copy)]
struct BlitSamplers<'a> {
    linear: &'a wgpu::Sampler,
    nearest: &'a wgpu::Sampler,
}

#[cfg(feature = "dev-diagnostics")]
struct RfAbGpu {
    _camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    target_1x: WorldTarget,
    target_4x: WorldTarget,
    target_200: WorldTarget,
    target_400: WorldTarget,
}

/// Client-only wgpu renderer. Not a reusable engine layer.
pub struct Renderer {
    text: super::text::TextRenderer,
    text_demo: super::text::TextContent,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline_1x: wgpu::RenderPipeline,
    pipeline_4x: Option<wgpu::RenderPipeline>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    draw_runs: Vec<DrawRun>,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    camera: Camera,
    blit_pipeline: wgpu::RenderPipeline,
    blit_bind_group_layout: wgpu::BindGroupLayout,
    blit_sampler: wgpu::Sampler,
    blit_sampler_nearest: wgpu::Sampler,
    #[cfg(feature = "dev-diagnostics")]
    camera_bind_group_layout: wgpu::BindGroupLayout,
    sprite_textures: Vec<SpriteTextureGpu>,
    world_target: Option<WorldTarget>,
    world_msaa: WorldMsaa,
    msaa_4x_supported: bool,
    render_scale: RenderScale,
    max_texture_dimension_2d: u32,
    internal_clamped: bool,
    adapter_name: String,
    backend: wgpu::Backend,
    frames_drawn: u64,
    #[cfg(feature = "dev-diagnostics")]
    rf_ab: Option<RfAbGpu>,
    #[cfg(feature = "dev-diagnostics")]
    rf_ab_elapsed: Option<f32>,
}

impl Renderer {
    pub fn new(window: Arc<Window>, resources: &[SpriteResource]) -> Result<Self, String> {
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
        let sprite_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("purgatory-sprite-bgl"),
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
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("purgatory-primitive-layout"),
            bind_group_layouts: &[
                Some(&camera_bind_group_layout),
                Some(&sprite_bind_group_layout),
            ],
            immediate_size: 0,
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERTEX_ATTRIBUTES,
        };

        let pipeline_1x = create_primitive_pipeline(
            &device,
            &shader,
            &pipeline_layout,
            vertex_layout.clone(),
            1,
            "purgatory-primitive-pipeline-1x",
        );
        let format_flags = adapter
            .get_texture_format_features(WORLD_TARGET_FORMAT)
            .flags;
        let msaa_4x_supported = world_format_supports_4x_msaa(format_flags);
        let pipeline_4x = if msaa_4x_supported {
            Some(create_primitive_pipeline(
                &device,
                &shader,
                &pipeline_layout,
                vertex_layout,
                WORLD_MSAA_X4,
                "purgatory-primitive-pipeline-4x",
            ))
        } else {
            None
        };
        let world_msaa = if msaa_4x_supported {
            WorldMsaa::X4
        } else {
            WorldMsaa::Off
        };

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
        let mut sprite_textures = Vec::with_capacity(resources.len());
        for resource in resources {
            sprite_textures.push(create_sprite_texture(
                &device,
                &queue,
                &sprite_bind_group_layout,
                resource.id,
                &format!("purgatory-sprite-{}", resource.id.raw()),
                &resource.image,
            )?);
        }

        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("purgatory-world-blit-shader"),
            source: wgpu::ShaderSource::Wgsl(BLIT_SHADER.into()),
        });
        let blit_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("purgatory-world-blit-bgl"),
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
            });
        let blit_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("purgatory-world-blit-layout"),
            bind_group_layouts: &[Some(&blit_bind_group_layout)],
            immediate_size: 0,
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("purgatory-world-blit-pipeline"),
            layout: Some(&blit_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blit_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: None,
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
        let blit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("purgatory-world-blit-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let blit_sampler_nearest = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("purgatory-world-blit-nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let blit_samplers = BlitSamplers {
            linear: &blit_sampler,
            nearest: &blit_sampler_nearest,
        };
        let max_texture_dimension_2d = device.limits().max_texture_dimension_2d;
        let render_scale = RenderScale::DEFAULT;
        let (world_target, internal_clamped) = match constrained_pixel_viewport(width, height)
            .and_then(|vp| {
                internal_render_size(vp.width, vp.height, render_scale, max_texture_dimension_2d)
            }) {
            Some(size) => (
                Some(create_world_target(
                    &device,
                    &blit_bind_group_layout,
                    blit_samplers,
                    size.width,
                    size.height,
                    world_msaa.sample_count(),
                )),
                size.clamped,
            ),
            None => (None, false),
        };

        println!(
            "PURGATORY renderer initialized adapter='{}' backend={:?} device_type={:?} format={:?} size={}x{} world_msaa={} 4x_supported={}",
            info.name,
            info.backend,
            info.device_type,
            config.format,
            width,
            height,
            world_msaa.as_str(),
            msaa_4x_supported
        );

        let text = super::text::TextRenderer::new(&device, config.format)?;
        Ok(Self {
            text,
            text_demo: super::text::TextContent(String::from(
                "PURGATORY — Text v0\nNative UI text online",
            )),
            surface,
            device,
            queue,
            config,
            pipeline_1x,
            pipeline_4x,
            vertex_buffer,
            index_buffer,
            draw_runs: Vec::new(),
            camera_buffer,
            camera_bind_group,
            camera,
            blit_pipeline,
            blit_bind_group_layout,
            blit_sampler,
            blit_sampler_nearest,
            #[cfg(feature = "dev-diagnostics")]
            camera_bind_group_layout,
            sprite_textures,
            world_target,
            world_msaa,
            msaa_4x_supported,
            render_scale,
            max_texture_dimension_2d,
            internal_clamped,
            adapter_name: info.name,
            backend: info.backend,
            frames_drawn: 0,
            #[cfg(feature = "dev-diagnostics")]
            rf_ab: None,
            #[cfg(feature = "dev-diagnostics")]
            rf_ab_elapsed: None,
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
        self.max_texture_dimension_2d
    }

    #[must_use]
    pub fn surface_size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Pixel rectangle used for the world pass (letterbox / pillarbox).
    #[must_use]
    pub fn gameplay_pixel_viewport(&self) -> Option<PixelViewport> {
        constrained_pixel_viewport(self.config.width, self.config.height)
    }

    #[must_use]
    pub fn render_scale(&self) -> RenderScale {
        self.render_scale
    }

    #[must_use]
    pub fn internal_render_extent(&self) -> Option<(u32, u32)> {
        self.world_target.as_ref().map(|target| target.size)
    }

    #[must_use]
    pub fn internal_render_clamped(&self) -> bool {
        self.internal_clamped
    }

    /// Change internal world resolution. Does not resize the window or camera FOV.
    pub fn set_render_scale(&mut self, scale: RenderScale) {
        if self.render_scale == scale {
            return;
        }
        self.render_scale = scale;
        self.ensure_world_target();
    }

    #[must_use]
    pub fn world_msaa(&self) -> WorldMsaa {
        self.world_msaa
    }

    #[must_use]
    pub fn msaa_4x_supported(&self) -> bool {
        self.msaa_4x_supported
    }

    #[must_use]
    pub fn world_msaa_sample_count(&self) -> u32 {
        self.world_target
            .as_ref()
            .map(|target| target.sample_count)
            .unwrap_or_else(|| self.applied_msaa().sample_count())
    }

    fn applied_msaa(&self) -> WorldMsaa {
        if self.world_msaa == WorldMsaa::X4 && !self.msaa_4x_supported {
            WorldMsaa::Off
        } else {
            self.world_msaa
        }
    }

    /// DEV comparison: 1× vs 4× coverage. No camera FOV or visibility change.
    /// Returns whether the applied sample count changed.
    pub fn set_world_msaa(&mut self, mode: WorldMsaa) -> bool {
        let requested = if mode == WorldMsaa::X4 && !self.msaa_4x_supported {
            eprintln!(
                "PURGATORY renderer: 4× MSAA unsupported for {:?}; staying {}",
                WORLD_TARGET_FORMAT,
                self.applied_msaa().as_str()
            );
            return false;
        } else {
            mode
        };
        if self.world_msaa == requested {
            return false;
        }
        let before = self.applied_msaa();
        self.world_msaa = requested;
        self.ensure_world_target();
        before != self.applied_msaa()
    }

    /// DEV RF1.5/RF2 compositor. `None` disables. Does not change gameplay Render Scale.
    #[cfg(feature = "dev-diagnostics")]
    pub fn set_rf_ab_proof(&mut self, elapsed: Option<f32>) {
        self.rf_ab_elapsed = elapsed.filter(|t| t.is_finite());
        if self.rf_ab_elapsed.is_some() {
            self.ensure_rf_ab();
        }
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

    /// Reconfigure the swapchain when the framebuffer size actually changes.
    /// Zero-size (minimized) and duplicate sizes are no-ops.
    pub fn resize(&mut self, width: u32, height: u32) {
        match classify_framebuffer_resize((self.config.width, self.config.height), (width, height))
        {
            SurfaceResizeAction::SkipInvalid | SurfaceResizeAction::Unchanged => {}
            SurfaceResizeAction::Reconfigure { width, height } => {
                self.config.width = width;
                self.config.height = height;
                self.surface.configure(&self.device, &self.config);
                // World FOV is independent of framebuffer size. Letterbox is
                // applied at blit time. Recreate the world target only if the
                // gameplay rect's pixel size changed.
                self.ensure_world_target();
            }
        }
    }

    pub fn reconfigure(&mut self) {
        if is_usable_surface(self.config.width, self.config.height) {
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn render(
        &mut self,
        world_quads: &[DrawQuad],
        show_ui_text: bool,
        overlay: impl FnOnce(OverlayPass<'_>) -> Vec<wgpu::CommandBuffer>,
    ) -> FrameStatus {
        if !is_usable_surface(self.config.width, self.config.height) {
            return FrameStatus::Skipped;
        }
        self.upload_quads(world_quads);

        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                let status = self.draw_surface_texture(texture, show_ui_text, overlay);
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

        self.draw_surface_texture(surface_texture, show_ui_text, overlay)
    }

    fn draw_surface_texture(
        &mut self,
        surface_texture: wgpu::SurfaceTexture,
        show_ui_text: bool,
        overlay: impl FnOnce(OverlayPass<'_>) -> Vec<wgpu::CommandBuffer>,
    ) -> FrameStatus {
        self.ensure_world_target();
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("purgatory-frame"),
            });

        if let Some(target) = &self.world_target {
            let color_view = target.msaa_view.as_ref().unwrap_or(&target.view);
            let resolve_target = if target.msaa_view.is_some() {
                Some(&target.view)
            } else {
                None
            };
            let store = if target.msaa_view.is_some() {
                wgpu::StoreOp::Discard
            } else {
                wgpu::StoreOp::Store
            };
            let pipeline = if target.sample_count == WORLD_MSAA_X4 {
                self.pipeline_4x.as_ref().unwrap_or(&self.pipeline_1x)
            } else {
                &self.pipeline_1x
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("purgatory-world-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(LETTERBOX_CLEAR),
                        store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            for run in &self.draw_runs {
                // Colored runs still bind a valid sprite resource because the
                // shader layout requires one. The vertex flag keeps them
                // untextured.
                let Some(texture) = run
                    .texture
                    .or_else(|| self.sprite_textures.first().map(|sprite| sprite.id))
                else {
                    continue;
                };
                let Some(sprite) = self
                    .sprite_textures
                    .iter()
                    .find(|sprite| sprite.id == texture)
                else {
                    continue;
                };
                pass.set_bind_group(1, &sprite.bind_group, &[]);
                let first_index = (run.first_quad * 6) as u32;
                let index_count = (run.quad_count * 6) as u32;
                pass.draw_indexed(first_index..first_index + index_count, 0, 0..1);
            }
        }

        #[cfg(feature = "dev-diagnostics")]
        self.encode_rf_ab(&mut encoder);

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("purgatory-present-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(LETTERBOX_CLEAR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let (Some(target), Some(vp)) = (
                self.world_target.as_ref(),
                constrained_pixel_viewport(self.config.width, self.config.height),
            ) {
                pass.set_pipeline(&self.blit_pipeline);
                pass.set_bind_group(0, &target.bind_group, &[]);
                pass.set_viewport(
                    vp.x as f32,
                    vp.y as f32,
                    vp.width as f32,
                    vp.height as f32,
                    0.0,
                    1.0,
                );
                pass.set_scissor_rect(vp.x, vp.y, vp.width, vp.height);
                pass.draw(0..3, 0..1);
            }
            #[cfg(feature = "dev-diagnostics")]
            self.blit_rf_ab_panels(&mut pass);
        }

        if show_ui_text {
            self.text.prepare(
                &self.queue,
                &self.text_demo,
                super::text::TextStyle {
                    font_size: 24.0,
                    color: [0.85, 0.92, 1.0, 1.0],
                    alignment: super::text::Alignment::Center,
                },
                [self.config.width as f32 * 0.5, 24.0],
                [self.config.width, self.config.height],
            );
            self.text.draw(&mut encoder, &view);
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

    fn ensure_world_target(&mut self) {
        let Some(vp) = self.gameplay_pixel_viewport() else {
            return;
        };
        let Some(size) = internal_render_size(
            vp.width,
            vp.height,
            self.render_scale,
            self.max_texture_dimension_2d,
        ) else {
            return;
        };
        self.internal_clamped = size.clamped;
        let sample_count = self.applied_msaa().sample_count();
        let configured = self
            .world_target
            .as_ref()
            .map(|target| (target.size, target.sample_count));
        let size_action = classify_world_target_resize(
            configured.map(|(size, _)| size),
            (size.width, size.height),
        );
        let sample_changed = configured.is_some_and(|(_, samples)| samples != sample_count);
        match size_action {
            WorldTargetAction::SkipInvalid => {}
            WorldTargetAction::Unchanged if !sample_changed => {}
            WorldTargetAction::Unchanged | WorldTargetAction::Recreate { .. } => {
                self.world_target = Some(create_world_target(
                    &self.device,
                    &self.blit_bind_group_layout,
                    BlitSamplers {
                        linear: &self.blit_sampler,
                        nearest: &self.blit_sampler_nearest,
                    },
                    size.width,
                    size.height,
                    sample_count,
                ));
            }
        }
    }

    fn write_camera_uniform(&self) {
        let uniform = CameraUniform::from_camera(&self.camera);
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    fn upload_quads(&mut self, world_quads: &[DrawQuad]) {
        let n = world_quads.len().min(MAX_QUADS);
        let mut vertices = Vec::with_capacity(n * 4);
        for quad in world_quads.iter().take(n) {
            vertices.extend_from_slice(&quad_vertices(*quad));
        }
        if !vertices.is_empty() {
            self.queue
                .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        }
        self.draw_runs = build_draw_runs(world_quads);
    }

    #[cfg(feature = "dev-diagnostics")]
    fn ensure_rf_ab(&mut self) {
        if self.rf_ab.is_some() {
            return;
        }
        let samples_4x = if self.msaa_4x_supported {
            WORLD_MSAA_X4
        } else {
            1
        };
        let src_1 = rf_ab_panel_source(100);
        let src_200 = rf_ab_panel_source(200);
        let src_400 = rf_ab_panel_source(400);
        let samplers = BlitSamplers {
            linear: &self.blit_sampler,
            nearest: &self.blit_sampler_nearest,
        };
        let target_1x = create_world_target(
            &self.device,
            &self.blit_bind_group_layout,
            samplers,
            src_1.0,
            src_1.1,
            1,
        );
        let target_4x = create_world_target(
            &self.device,
            &self.blit_bind_group_layout,
            samplers,
            src_1.0,
            src_1.1,
            samples_4x,
        );
        let target_200 = create_world_target(
            &self.device,
            &self.blit_bind_group_layout,
            samplers,
            src_200.0,
            src_200.1,
            samples_4x,
        );
        let target_400 = create_world_target(
            &self.device,
            &self.blit_bind_group_layout,
            samplers,
            src_400.0,
            src_400.1,
            samples_4x,
        );
        let camera = rf_ab_camera();
        let camera_uniform = CameraUniform::from_camera(&camera);
        let camera_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("purgatory-rf-ab-camera"),
            size: std::mem::size_of::<CameraUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue
            .write_buffer(&camera_buffer, 0, bytemuck::bytes_of(&camera_uniform));
        let camera_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("purgatory-rf-ab-camera-bg"),
            layout: &self.camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("purgatory-rf-ab-vertices"),
            size: (4 * 4 * std::mem::size_of::<Vertex>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.rf_ab = Some(RfAbGpu {
            _camera_buffer: camera_buffer,
            camera_bind_group,
            vertex_buffer,
            target_1x,
            target_4x,
            target_200,
            target_400,
        });
    }

    #[cfg(feature = "dev-diagnostics")]
    fn encode_rf_ab(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let Some(elapsed) = self.rf_ab_elapsed else {
            return;
        };
        self.ensure_rf_ab();
        let Some(rf) = self.rf_ab.as_ref() else {
            return;
        };
        let quads = rf_ab_scene_quads(elapsed);
        let mut vertices = Vec::with_capacity(quads.len() * 4);
        for quad in quads {
            vertices.extend_from_slice(&quad_vertices(quad));
        }
        self.queue
            .write_buffer(&rf.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        let index_count = (quads.len() * 6) as u32;
        let sprite_bind_group = &self.sprite_textures[0].bind_group;
        let index_buffer = &self.index_buffer;
        let draw_1x = ColoredPassDraw {
            pipeline: &self.pipeline_1x,
            camera_bind_group: &rf.camera_bind_group,
            sprite_bind_group,
            vertex_buffer: &rf.vertex_buffer,
            index_buffer,
            index_count,
        };
        encode_colored_target(encoder, &rf.target_1x, draw_1x, "purgatory-rf-ab-1x");
        let pipeline_4x = self.pipeline_4x.as_ref().unwrap_or(&self.pipeline_1x);
        let draw_4x = ColoredPassDraw {
            pipeline: pipeline_4x,
            camera_bind_group: &rf.camera_bind_group,
            sprite_bind_group,
            vertex_buffer: &rf.vertex_buffer,
            index_buffer,
            index_count,
        };
        encode_colored_target(encoder, &rf.target_4x, draw_4x, "purgatory-rf-ab-4x");
        let draw_200 = ColoredPassDraw {
            pipeline: pipeline_4x,
            camera_bind_group: &rf.camera_bind_group,
            sprite_bind_group,
            vertex_buffer: &rf.vertex_buffer,
            index_buffer,
            index_count,
        };
        encode_colored_target(encoder, &rf.target_200, draw_200, "purgatory-rf-ab-200");
        let draw_400 = ColoredPassDraw {
            pipeline: pipeline_4x,
            camera_bind_group: &rf.camera_bind_group,
            sprite_bind_group,
            vertex_buffer: &rf.vertex_buffer,
            index_buffer,
            index_count,
        };
        encode_colored_target(encoder, &rf.target_400, draw_400, "purgatory-rf-ab-400");
    }

    #[cfg(feature = "dev-diagnostics")]
    fn blit_rf_ab_panels(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(rf) = self.rf_ab.as_ref() else {
            return;
        };
        if self.rf_ab_elapsed.is_none() {
            return;
        }
        let fb_w = self.config.width;
        let fb_h = self.config.height;
        for panel in rf_ab_layout(self.msaa_4x_supported) {
            let bind_group = rf.blit_bind_group(panel);
            blit_panel(
                pass,
                &self.blit_pipeline,
                bind_group,
                panel.dest,
                fb_w,
                fb_h,
            );
        }
    }
}

struct SpriteTextureGpu {
    id: SpriteTextureId,
    _texture: wgpu::Texture,
    _sampler: wgpu::Sampler,
    bind_group: wgpu::BindGroup,
}

fn create_sprite_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    id: SpriteTextureId,
    label: &str,
    img: &image::RgbaImage,
) -> Result<SpriteTextureGpu, String> {
    let width = img.width();
    let height = img.height();
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
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
        img.as_raw(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        size,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("purgatory-sprite-nearest"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("purgatory-sprite-bg"),
        layout,
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
    Ok(SpriteTextureGpu {
        id,
        _texture: texture,
        _sampler: sampler,
        bind_group,
    })
}

fn create_primitive_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    vertex_layout: wgpu::VertexBufferLayout<'_>,
    sample_count: u32,
    label: &str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(vertex_layout)],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: WORLD_TARGET_FORMAT,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: sample_count,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    })
}

fn create_world_target(
    device: &wgpu::Device,
    blit_layout: &wgpu::BindGroupLayout,
    samplers: BlitSamplers<'_>,
    width: u32,
    height: u32,
    sample_count: u32,
) -> WorldTarget {
    let extent = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let color_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("purgatory-world-target"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: WORLD_TARGET_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let (msaa_texture, msaa_view) = if sample_count > 1 {
        let msaa_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("purgatory-world-msaa"),
            size: extent,
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: WORLD_TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let msaa_view = msaa_texture.create_view(&wgpu::TextureViewDescriptor::default());
        (Some(msaa_texture), Some(msaa_view))
    } else {
        (None, None)
    };
    let bind_group = make_blit_bind_group(
        device,
        blit_layout,
        &view,
        samplers.linear,
        "purgatory-world-blit-bg",
    );
    let bind_group_nearest = make_blit_bind_group(
        device,
        blit_layout,
        &view,
        samplers.nearest,
        "purgatory-world-blit-nearest-bg",
    );
    WorldTarget {
        _msaa_texture: msaa_texture,
        _color_texture: color_texture,
        msaa_view,
        view,
        bind_group,
        bind_group_nearest,
        size: (width, height),
        sample_count,
    }
}

fn make_blit_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    label: &'static str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

#[cfg(feature = "dev-diagnostics")]
struct ColoredPassDraw<'a> {
    pipeline: &'a wgpu::RenderPipeline,
    camera_bind_group: &'a wgpu::BindGroup,
    sprite_bind_group: &'a wgpu::BindGroup,
    vertex_buffer: &'a wgpu::Buffer,
    index_buffer: &'a wgpu::Buffer,
    index_count: u32,
}

#[cfg(feature = "dev-diagnostics")]
fn encode_colored_target(
    encoder: &mut wgpu::CommandEncoder,
    target: &WorldTarget,
    draw: ColoredPassDraw<'_>,
    label: &'static str,
) {
    let color_view = target.msaa_view.as_ref().unwrap_or(&target.view);
    let resolve_target = if target.msaa_view.is_some() {
        Some(&target.view)
    } else {
        None
    };
    let store = if target.msaa_view.is_some() {
        wgpu::StoreOp::Discard
    } else {
        wgpu::StoreOp::Store
    };
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: color_view,
            depth_slice: None,
            resolve_target,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(LETTERBOX_CLEAR),
                store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(draw.pipeline);
    pass.set_bind_group(0, draw.camera_bind_group, &[]);
    pass.set_bind_group(1, draw.sprite_bind_group, &[]);
    pass.set_vertex_buffer(0, draw.vertex_buffer.slice(..));
    pass.set_index_buffer(draw.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
    if draw.index_count > 0 {
        pass.draw_indexed(0..draw.index_count, 0, 0..1);
    }
}

#[cfg(feature = "dev-diagnostics")]
fn blit_panel(
    pass: &mut wgpu::RenderPass<'_>,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
    dest: PixelViewport,
    fb_w: u32,
    fb_h: u32,
) {
    if dest.width == 0 || dest.height == 0 {
        return;
    }
    if dest.x.saturating_add(dest.width) > fb_w || dest.y.saturating_add(dest.height) > fb_h {
        return;
    }
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.set_viewport(
        dest.x as f32,
        dest.y as f32,
        dest.width as f32,
        dest.height as f32,
        0.0,
        1.0,
    );
    pass.set_scissor_rect(dest.x, dest.y, dest.width, dest.height);
    pass.draw(0..3, 0..1);
}

#[cfg(feature = "dev-diagnostics")]
impl RfAbGpu {
    fn blit_bind_group(&self, panel: RfAbPanel) -> &wgpu::BindGroup {
        match panel.slot {
            RfAbSlot::Msaa1x | RfAbSlot::BlitNearest => &self.target_1x.bind_group_nearest,
            RfAbSlot::BlitLinear => &self.target_1x.bind_group,
            RfAbSlot::Msaa4x | RfAbSlot::Scale100 => {
                if panel.nearest {
                    &self.target_4x.bind_group_nearest
                } else {
                    &self.target_4x.bind_group
                }
            }
            RfAbSlot::Scale200 => &self.target_200.bind_group,
            RfAbSlot::Scale400 => &self.target_400.bind_group,
        }
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
    let color = quad.color;
    let c = quad.world_corners();
    let textured = if quad.is_textured() { 1.0 } else { 0.0 };
    [
        Vertex {
            position: c[0],
            color,
            uv: quad.uvs[0],
            textured,
        },
        Vertex {
            position: c[1],
            color,
            uv: quad.uvs[1],
            textured,
        },
        Vertex {
            position: c[2],
            color,
            uv: quad.uvs[2],
            textured,
        },
        Vertex {
            position: c[3],
            color,
            uv: quad.uvs[3],
            textured,
        },
    ]
}

fn axis_aligned_corners(quad: DrawQuad) -> [[f32; 2]; 4] {
    let hx = quad.size[0] * 0.5;
    let hy = quad.size[1] * 0.5;
    let x = quad.center[0];
    let y = quad.center[1];
    if quad.triangle {
        return [[x - hx, y - hy], [x + hx, y - hy], [x, y + hy], [x, y + hy]];
    }
    [
        [x - hx, y - hy],
        [x + hx, y - hy],
        [x + hx, y + hy],
        [x - hx, y + hy],
    ]
}

fn oriented_corners(
    pivot: [f32; 2],
    size: [f32; 2],
    local_center: [f32; 2],
    rotation: f32,
) -> [[f32; 2]; 4] {
    let hx = size[0] * 0.5;
    let hy = size[1] * 0.5;
    let (sin, cos) = rotation.sin_cos();
    let locals = [
        [local_center[0] - hx, local_center[1] - hy],
        [local_center[0] + hx, local_center[1] - hy],
        [local_center[0] + hx, local_center[1] + hy],
        [local_center[0] - hx, local_center[1] + hy],
    ];
    [
        offset_pivot(pivot, rotate_local(locals[0], sin, cos)),
        offset_pivot(pivot, rotate_local(locals[1], sin, cos)),
        offset_pivot(pivot, rotate_local(locals[2], sin, cos)),
        offset_pivot(pivot, rotate_local(locals[3], sin, cos)),
    ]
}

fn rotate_local(p: [f32; 2], sin: f32, cos: f32) -> [f32; 2] {
    [cos * p[0] - sin * p[1], sin * p[0] + cos * p[1]]
}

fn offset_pivot(pivot: [f32; 2], local: [f32; 2]) -> [f32; 2] {
    [pivot[0] + local[0], pivot[1] + local[1]]
}

fn convex_corners(pivot: [f32; 2], local_corners: [[f32; 2]; 4], rotation: f32) -> [[f32; 2]; 4] {
    let (sin, cos) = rotation.sin_cos();
    [
        offset_pivot(pivot, rotate_local(local_corners[0], sin, cos)),
        offset_pivot(pivot, rotate_local(local_corners[1], sin, cos)),
        offset_pivot(pivot, rotate_local(local_corners[2], sin, cos)),
        offset_pivot(pivot, rotate_local(local_corners[3], sin, cos)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-5;

    fn corners_eq(a: [[f32; 2]; 4], b: [[f32; 2]; 4]) -> bool {
        a.iter()
            .zip(b.iter())
            .all(|(p, q)| (p[0] - q[0]).abs() < EPS && (p[1] - q[1]).abs() < EPS)
    }

    #[test]
    fn textured_sprite_matches_convex_corners() {
        let pivot = [2.0, -1.0];
        let locals = [[-0.5, -0.3], [0.5, -0.3], [0.5, 0.7], [-0.5, 0.7]];
        let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
        let solid = DrawQuad::convex(pivot, locals, 0.25, [1.0; 4]);
        let sprite = DrawQuad::textured_sprite(SpriteTextureId::HEADWEAR, pivot, locals, uvs, 0.25);
        assert!(sprite.is_textured());
        assert_eq!(sprite.sprite_texture_id(), Some(SpriteTextureId::HEADWEAR));
        assert!(!solid.is_textured());
        assert!(corners_eq(sprite.world_corners(), solid.world_corners()));
        assert_eq!(sprite.color, [1.0, 1.0, 1.0, 1.0]);
    }

    fn test_sprite(texture: SpriteTextureId) -> DrawQuad {
        DrawQuad::textured_sprite(
            texture,
            [0.0, 0.0],
            [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            0.0,
        )
    }

    #[test]
    fn draw_runs_preserve_a_b_a_order() {
        let a = SpriteTextureId::from_raw(1);
        let b = SpriteTextureId::from_raw(2);
        let runs = build_draw_runs(&[test_sprite(a), test_sprite(b), test_sprite(a)]);
        assert_eq!(
            runs,
            [
                DrawRun {
                    texture: Some(a),
                    first_quad: 0,
                    quad_count: 1
                },
                DrawRun {
                    texture: Some(b),
                    first_quad: 1,
                    quad_count: 1
                },
                DrawRun {
                    texture: Some(a),
                    first_quad: 2,
                    quad_count: 1
                }
            ]
        );
    }

    #[test]
    fn draw_runs_keep_colored_quads_in_sequence() {
        let a = SpriteTextureId::from_raw(1);
        let runs = build_draw_runs(&[
            test_sprite(a),
            DrawQuad::rect([0.0, 0.0], [1.0, 1.0], [1.0; 4]),
            test_sprite(a),
        ]);
        assert_eq!(
            runs.iter().map(|run| run.texture).collect::<Vec<_>>(),
            vec![Some(a), None, Some(a)]
        );
        assert_eq!(
            runs.iter().map(|run| run.first_quad).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn draw_runs_merge_consecutive_same_texture() {
        let a = SpriteTextureId::from_raw(1);
        let runs = build_draw_runs(&[test_sprite(a), test_sprite(a), test_sprite(a)]);
        assert_eq!(
            runs,
            [DrawRun {
                texture: Some(a),
                first_quad: 0,
                quad_count: 3
            }]
        );
    }

    #[test]
    fn draw_runs_truncate_at_max_quads() {
        let a = SpriteTextureId::from_raw(1);
        let b = SpriteTextureId::from_raw(2);
        let mut quads = vec![test_sprite(a); MAX_QUADS];
        quads.push(test_sprite(b));
        let runs = build_draw_runs(&quads);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].first_quad, 0);
        assert_eq!(runs[0].quad_count, MAX_QUADS);
    }

    #[test]
    fn colored_quad_has_no_sprite_texture_identity() {
        let colored = DrawQuad::rect([0.0, 0.0], [1.0, 1.0], [1.0; 4]);
        assert_eq!(colored.sprite_texture_id(), None);
    }

    #[test]
    fn axis_aligned_rect_corners_match_legacy_aabb() {
        let q = DrawQuad::rect([1.0, -2.0], [4.0, 6.0], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(
            q.world_corners(),
            [[-1.0, -5.0], [3.0, -5.0], [3.0, 1.0], [-1.0, 1.0]]
        );
    }

    #[test]
    fn axis_aligned_triangle_corners_match_legacy_peak() {
        let q = DrawQuad::triangle([0.0, 0.0], [2.0, 4.0], [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(
            q.world_corners(),
            [[-1.0, -2.0], [1.0, -2.0], [0.0, 2.0], [0.0, 2.0]]
        );
    }

    #[test]
    fn oriented_rotation_zero_is_pivot_plus_local_center() {
        let pivot = [3.0, 1.0];
        let local_center = [0.0, -0.05];
        let size = [0.08, 0.10];
        let q = DrawQuad::oriented(pivot, size, local_center, 0.0, [1.0; 4]);
        let hx = 0.04;
        let hy = 0.05;
        let expected = [
            [
                pivot[0] + local_center[0] - hx,
                pivot[1] + local_center[1] - hy,
            ],
            [
                pivot[0] + local_center[0] + hx,
                pivot[1] + local_center[1] - hy,
            ],
            [
                pivot[0] + local_center[0] + hx,
                pivot[1] + local_center[1] + hy,
            ],
            [
                pivot[0] + local_center[0] - hx,
                pivot[1] + local_center[1] + hy,
            ],
        ];
        assert!(corners_eq(q.world_corners(), expected));
    }

    #[test]
    fn oriented_90_ccw_around_non_central_pivot() {
        let pivot = [1.0, 2.0];
        let local_center = [0.0, -0.05];
        let size = [0.08, 0.10];
        let q = DrawQuad::oriented(
            pivot,
            size,
            local_center,
            std::f32::consts::FRAC_PI_2,
            [1.0; 4],
        );
        // local (x, y) -> (-y, x) at 90° CCW, then + pivot.
        let expected = [[1.10, 1.96], [1.10, 2.04], [1.00, 2.04], [1.00, 1.96]];
        assert!(corners_eq(q.world_corners(), expected));
    }

    #[test]
    fn oriented_rotation_preserves_pivot() {
        let pivot = [4.0, -1.0];
        let local_center = [0.2, -0.3];
        let size = [0.5, 0.8];
        let a = DrawQuad::oriented(pivot, size, local_center, 0.4, [1.0; 4]).world_corners();
        let b = DrawQuad::oriented(pivot, size, local_center, -1.1, [1.0; 4]).world_corners();
        for corners in [a, b] {
            for p in corners {
                let dx = p[0] - pivot[0];
                let dy = p[1] - pivot[1];
                let dist = (dx * dx + dy * dy).sqrt();
                assert!(dist > 0.01);
            }
        }
        let dist = |c: [[f32; 2]; 4]| {
            c.map(|p| {
                let dx = p[0] - pivot[0];
                let dy = p[1] - pivot[1];
                (dx * dx + dy * dy).sqrt()
            })
        };
        let da = dist(a);
        let db = dist(b);
        for i in 0..4 {
            assert!((da[i] - db[i]).abs() < EPS);
        }
    }

    #[test]
    fn convex_zero_rotation_is_pivot_plus_local_corners() {
        let pivot = [2.0, -1.0];
        let locals = [[-0.2, -0.3], [0.1, -0.3], [0.25, 0.4], [-0.25, 0.4]];
        let q = DrawQuad::convex(pivot, locals, 0.0, [1.0; 4]);
        let expected = [
            [pivot[0] + locals[0][0], pivot[1] + locals[0][1]],
            [pivot[0] + locals[1][0], pivot[1] + locals[1][1]],
            [pivot[0] + locals[2][0], pivot[1] + locals[2][1]],
            [pivot[0] + locals[3][0], pivot[1] + locals[3][1]],
        ];
        assert!(corners_eq(q.world_corners(), expected));
        assert!(!q.triangle);
    }

    #[test]
    fn convex_rotates_around_supplied_pivot() {
        let pivot = [1.0, 2.0];
        let locals = [[-0.2, -0.4], [0.1, -0.4], [0.3, 0.2], [-0.3, 0.2]];
        let q = DrawQuad::convex(pivot, locals, std::f32::consts::FRAC_PI_2, [1.0; 4]);
        // local (x, y) -> (-y, x) at 90° CCW, then + pivot.
        let expected = [[1.4, 1.8], [1.4, 2.1], [0.8, 2.3], [0.8, 1.7]];
        assert!(corners_eq(q.world_corners(), expected));
        for p in q.world_corners() {
            let dx = p[0] - pivot[0];
            let dy = p[1] - pivot[1];
            assert!(dx * dx + dy * dy > 0.01);
        }
    }

    #[test]
    fn oriented_rect_is_unchanged_beside_convex() {
        let pivot = [0.0, 0.0];
        let size = [0.08, 0.10];
        let local_center = [0.0, -0.05];
        let oriented = DrawQuad::oriented(pivot, size, local_center, 0.3, [1.0; 4]);
        let again = DrawQuad::oriented(pivot, size, local_center, 0.3, [1.0; 4]);
        assert!(corners_eq(oriented.world_corners(), again.world_corners()));
        let _ = DrawQuad::convex(
            pivot,
            [[-0.2, -0.3], [0.2, -0.3], [0.3, 0.3], [-0.3, 0.3]],
            0.3,
            [1.0; 4],
        );
        assert!(corners_eq(oriented.world_corners(), again.world_corners()));
    }

    fn corners_aabb(corners: [[f32; 2]; 4]) -> ([f32; 2], [f32; 2]) {
        let mut min = [f32::INFINITY, f32::INFINITY];
        let mut max = [f32::NEG_INFINITY, f32::NEG_INFINITY];
        for p in corners {
            min[0] = min[0].min(p[0]);
            min[1] = min[1].min(p[1]);
            max[0] = max[0].max(p[0]);
            max[1] = max[1].max(p[1]);
        }
        (min, max)
    }

    #[test]
    fn limb_world_and_output_bounds_ignore_render_scale() {
        use super::super::camera::{Camera, constrained_pixel_viewport};
        use crate::display::{RenderScale, internal_render_size};

        let camera = Camera::footnote_test_dev();
        let gameplay = constrained_pixel_viewport(1920, 1080).expect("16:9");
        let matrix = camera.view_proj_column_major();
        // Matches `skeleton_debug` ARM_PLACEHOLDER_WIDTH × CHARACTER_VISUAL_SCALE_115.
        let width = 0.0944 * 1.15;
        let length = 0.40;
        let quad = DrawQuad::oriented(
            [0.0, 0.0],
            [width, length],
            [0.0, -length * 0.5],
            0.0,
            [1.0; 4],
        );
        let world = quad.world_corners();
        let (wmin, wmax) = corners_aabb(world);
        let output = camera.world_aabb_to_gameplay_px(wmin, wmax, gameplay);
        let scales = [RenderScale::P50, RenderScale::P100, RenderScale::P200];
        let mut internals = Vec::new();
        for scale in scales {
            let internal =
                internal_render_size(gameplay.width, gameplay.height, scale, 8192).expect("valid");
            internals.push((internal.width, internal.height));
            assert_eq!(camera.view_proj_column_major(), matrix);
            assert!(corners_eq(quad.world_corners(), world));
            assert!((wmax[0] - wmin[0] - width).abs() < 1e-4);
            let now = camera.world_aabb_to_gameplay_px(wmin, wmax, gameplay);
            for i in 0..4 {
                assert!(
                    (output[i] - now[i]).abs() < 1e-4,
                    "output bounds moved at {scale:?}: {output:?} vs {now:?}"
                );
            }
            let internal_px = width * (internal.width as f32 / camera.viewport_width);
            assert!(
                internal_px > 0.5,
                "limb vanished from the internal grid at {scale:?}"
            );
        }
        assert_ne!(internals[0], internals[1]);
        assert_ne!(internals[1], internals[2]);
        let sampling_50 = width * (internals[0].0 as f32 / camera.viewport_width);
        let sampling_100 = width * (internals[1].0 as f32 / camera.viewport_width);
        let sampling_200 = width * (internals[2].0 as f32 / camera.viewport_width);
        assert!((sampling_100 - 2.0 * sampling_50).abs() < 1e-3);
        assert!((sampling_200 - 4.0 * sampling_50).abs() < 1e-3);
        assert!((sampling_200 - 2.0 * sampling_100).abs() < 1e-3);
        assert_eq!(WorldMsaa::Off.sample_count(), 1);
        assert_eq!(WorldMsaa::X4.sample_count(), 4);
        assert_eq!(WORLD_MSAA_X4, 4);
        assert!(world_format_supports_4x_msaa(
            wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4
                | wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE
        ));
        assert!(!world_format_supports_4x_msaa(
            wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4
        ));
    }
}
