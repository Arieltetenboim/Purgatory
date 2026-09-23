//! Small offscreen framebuffer checks; no window, server, or screenshot framework.
use super::*;
use crate::renderer::ui::{UiRect, UiRenderer};

const SIZE: [u32; 2] = [1024, 768];
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

fn gpu() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&Default::default())).expect("GPU adapter");
    pollster::block_on(adapter.request_device(&Default::default())).unwrap()
}
fn target(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("text-verification"),
        size: wgpu::Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}
fn clear(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
}
fn world_blit(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    scale: f32,
) {
    let world = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("text-test-world"),
        size: wgpu::Extent3d {
            width: (SIZE[0] as f32 * scale) as u32,
            height: (SIZE[1] as f32 * scale) as u32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let world_view = world.create_view(&Default::default());
    clear(encoder, &world_view);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/blit.wgsl").into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let sampler = device.create_sampler(&Default::default());
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&world_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(&pipeline);
    pass.set_bind_group(0, &group, &[]);
    pass.draw(0..3, 0..1);
}
fn readback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    mut encoder: wgpu::CommandEncoder,
) -> Vec<u8> {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(SIZE[0] * SIZE[1] * 4),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE[0] * 4),
                rows_per_image: Some(SIZE[1]),
            },
        },
        wgpu::Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let bytes = buffer.slice(..).get_mapped_range().unwrap().to_vec();
    buffer.unmap();
    bytes
}
fn block(text: &str, size: f32, anchor: [f32; 2], color: [f32; 4]) -> TextBlock {
    TextBlock {
        content: TextContent(text.into()),
        style: TextStyle::at_size(size, color, Alignment::Left),
        anchor,
        max_width: None,
    }
}
fn panel_renderer(device: &wgpu::Device) -> UiRenderer {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
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
    UiRenderer::new(device, FORMAT, &layout)
}

#[test]
#[ignore = "requires GPU; verifies actual text pixels, persistent batches and panel occlusion"]
fn renderer_repeated_text_preserves_order_and_color() {
    let (device, queue) = gpu();
    let mut text = TextRenderer::new(&device, &queue, FORMAT);
    let mut panels = panel_renderer(&device);
    let mut first = vec![
        block("FIRST", 32.0, [10.0, 10.0], [1.0, 0.0, 0.0, 1.0]),
        block("HIDDEN", 32.0, [10.0, 65.0], [1.0, 0.0, 0.0, 1.0]),
    ];
    // Distinct sizes/glyphs exceed the initial 256-square atlas. Keep every batch resident.
    for (i, size) in [12.0, 13.0, 14.0, 16.0, 18.0, 24.0, 32.0, 64.0, 96.0]
        .into_iter()
        .enumerate()
    {
        first.push(block(
            "ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz 0123456789",
            size,
            [10.0, 150.0 + i as f32 * 58.0],
            [1.0; 4],
        ));
    }
    let second = [
        block("SECOND", 32.0, [10.0, 65.0], [0.0, 1.0, 0.0, 1.0]),
        block("MMMM", 32.0, [500.0, 10.0], [0.5, 0.2, 0.0, 0.5]),
    ];
    let mut previous = None;
    for world_scale in [1.0, 0.5] {
        text.begin_frame(&queue, SIZE);
        let a = text.prepare_blocks(&device, &queue, &first, 1.0).unwrap();
        let b = text.prepare_blocks(&device, &queue, &second, 1.0).unwrap();
        assert_eq!(text.metrics(b).count(), 2);
        assert_eq!(text.batches.len(), 2);
        let panel = panels.prepare(
            &device,
            &queue,
            &[UiRect {
                min: [0.0, 60.0],
                max: [400.0, 120.0],
                color: [0.0, 0.0, 1.0, 1.0],
            }],
            &[],
            SIZE,
        );
        let texture = target(&device);
        let view = texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        world_blit(&device, &mut encoder, &view, world_scale);
        text.draw(a, &mut encoder, &view);
        panels.draw(panel, &mut encoder, &view);
        text.draw(b, &mut encoder, &view);
        let bytes = readback(&device, &queue, &texture, encoder);
        let mut red_top = 0;
        let mut green_bottom = 0;
        let mut red_bottom = 0;
        let mut color_peak = [0u8; 3];
        for y in 0..120 {
            for x in 0..750 {
                let p = &bytes[((y * SIZE[0] + x) * 4) as usize..][..4];
                if x < 400 {
                    if y < 60 && p[0] > 200 {
                        red_top += 1;
                    }
                    if y >= 60 {
                        if p[1] > 200 {
                            green_bottom += 1;
                        }
                        if p[0] > 20 {
                            red_bottom += 1;
                        }
                    }
                } else if x >= 500 && y < 60 {
                    for c in 0..3 {
                        color_peak[c] = color_peak[c].max(p[c]);
                    }
                }
            }
        }
        assert!(red_top > 50 && green_bottom > 50);
        assert_eq!(red_bottom, 0);
        // Linear (0.5,0.2,0) at alpha 0.5 over black => sRGB approximately (137,89,0).
        assert!(
            (i32::from(color_peak[0]) - 137).abs() <= 3,
            "{color_peak:?}"
        );
        assert!((i32::from(color_peak[1]) - 89).abs() <= 3, "{color_peak:?}");
        assert_eq!(color_peak[2], 0);
        if let Some(previous) = previous {
            assert_eq!(bytes, previous);
        }
        previous = Some(bytes);
    }
}

#[test]
#[ignore = "development specimen: set PURGATORY_TEXT_SPECIMEN_DIR to export PNG framebuffers"]
fn renderer_repeated_text_visual_specimen() {
    let Some(directory) = std::env::var_os("PURGATORY_TEXT_SPECIMEN_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let (device, queue) = gpu();
    let mut text = TextRenderer::new(&device, &queue, FORMAT);
    for ui in [1.0, 1.25] {
        for os in [1.0, 1.5] {
            let scale = ui * os;
            let mut blocks = vec![block(
                &format!("UI {ui} / OS {os}: integer (left), fractional (right)"),
                18.0 / scale,
                [15.0, 8.0],
                [1.0; 4],
            )];
            for (row, size) in [12.0, 13.0, 14.0, 16.0, 18.0, 24.0, 32.0]
                .into_iter()
                .enumerate()
            {
                let label = format!("{size}px: Hamb 012 !? e\u{301}");
                for x in [15.0, 525.35] {
                    blocks.push(block(
                        &label,
                        size / scale,
                        [x, 45.0 + row as f32 * 58.0],
                        [1.0; 4],
                    ));
                }
            }
            blocks.push(block(
                "Color + opacity: minimum 012345",
                24.0 / scale,
                [15.0, 480.0],
                [0.5, 0.2, 0.05, 0.5],
            ));
            let mut wrapped = block(
                "Wrapped text: measurement and rendering share one shaped layout.\n\nAn empty line remains.",
                18.0 / scale,
                [15.0, 540.0],
                [0.4, 0.8, 1.0, 1.0],
            );
            wrapped.max_width = Some(310.0);
            blocks.push(wrapped);
            blocks.push(block(
                "Fixed 17 logical units",
                17.0,
                [525.0, 540.0],
                [1.0; 4],
            ));
            text.begin_frame(&queue, SIZE);
            let batch = text
                .prepare_blocks(&device, &queue, &blocks, scale)
                .unwrap();
            for (index, layout) in text.batches[batch].layouts.iter().enumerate() {
                assert!(
                    layout.buffer.layout_runs().any(|r| !r.glyphs.is_empty()),
                    "missing layout {index} at {scale}"
                );
            }
            let texture = target(&device);
            let view = texture.create_view(&Default::default());
            let mut encoder = device.create_command_encoder(&Default::default());
            clear(&mut encoder, &view);
            text.draw(batch, &mut encoder, &view);
            let bytes = readback(&device, &queue, &texture, encoder);
            let lit_row = (45..90)
                .flat_map(|y| (15..400).map(move |x| ((y * SIZE[0] + x) * 4) as usize))
                .filter(|i| bytes[*i] > 100)
                .count();
            assert!(
                lit_row > 30,
                "missing rendered size row at {scale}: {lit_row}"
            );
            image::save_buffer(
                directory.join(format!("text-ui-{ui}-os-{os}.png")),
                &bytes,
                SIZE[0],
                SIZE[1],
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
}
