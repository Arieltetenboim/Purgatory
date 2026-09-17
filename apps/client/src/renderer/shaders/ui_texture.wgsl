@group(0) @binding(0)
var ui_texture: texture_2d<f32>;

@group(0) @binding(1)
var ui_sampler: sampler;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
    @location(2) uv_min: vec2<f32>,
    @location(3) uv_max: vec2<f32>,
    @location(4) rect_min: vec2<f32>,
    @location(5) rect_size: vec2<f32>,
}

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) tint: vec4<f32>,
    @location(3) uv_min: vec2<f32>,
    @location(4) uv_max: vec2<f32>,
    @location(5) rect_min: vec2<f32>,
    @location(6) rect_size: vec2<f32>,
) -> Out {
    var out: Out;
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.uv = uv;
    out.tint = tint;
    out.uv_min = uv_min;
    out.uv_max = uv_max;
    out.rect_min = rect_min;
    out.rect_size = rect_size;
    return out;
}

@fragment
fn fs_main(input: Out) -> @location(0) vec4<f32> {
    let texture_size = vec2<f32>(textureDimensions(ui_texture));
    let source_size = max(
        round((input.uv_max - input.uv_min) * texture_size) + vec2<f32>(1.0),
        vec2<f32>(1.0),
    );
    let source_origin = input.uv_min * texture_size - vec2<f32>(0.5);
    let local = max(input.position.xy - input.rect_min, vec2<f32>(0.0));

    // Tiny source samples are intentional flat UI fills. Treat their tint as
    // the fill instead of magnifying atlas-boundary pixels across a panel.
    if (source_size.x <= 4.0 && source_size.y <= 4.0 &&
        (input.rect_size.x > source_size.x || input.rect_size.y > source_size.y)) {
        let center_uv = (input.uv_min + input.uv_max) * 0.5;
        return textureSample(ui_texture, ui_sampler, center_uv) * input.tint;
    }

    var sample_uv = input.uv;
    if (input.rect_size.x > source_size.x + 0.5) {
        let px = floor(min(local.x, max(input.rect_size.x - 1.0, 0.0)));
        let source_x = px - floor(px / source_size.x) * source_size.x;
        sample_uv.x = (source_origin.x + source_x + 0.5) / texture_size.x;
    }
    if (input.rect_size.y > source_size.y + 0.5) {
        let py = floor(min(local.y, max(input.rect_size.y - 1.0, 0.0)));
        let source_y = py - floor(py / source_size.y) * source_size.y;
        sample_uv.y = (source_origin.y + source_y + 0.5) / texture_size.y;
    }

    return textureSample(ui_texture, ui_sampler, sample_uv) * input.tint;
}
