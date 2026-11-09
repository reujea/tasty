pub(crate) const BG_SHADER: &str = r#"
struct Uniforms {
    cell_size: vec2<f32>,
    viewport_size: vec2<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct BgInstance {
    @location(0) pos: vec2<f32>,
    @location(1) viewport_offset: vec2<f32>,
    @location(2) bg_color: vec4<f32>,
    @location(3) line: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, instance: BgInstance) -> VertexOutput {
    let quad_pos = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0),
    );

    let p = quad_pos[vi];
    var in_cell = p * uniforms.cell_size;
    if instance.line.y > 0.0 {
        in_cell.y = instance.line.x + p.y * instance.line.y;
    }
    let pixel_pos = instance.pos * uniforms.cell_size + in_cell + instance.viewport_offset;
    let ndc = pixel_pos / uniforms.viewport_size * 2.0 - 1.0;

    var out: VertexOutput;
    out.position = vec4(ndc.x, -ndc.y, 0.0, 1.0);
    out.color = instance.bg_color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

pub(crate) const GLYPH_SHADER: &str = r#"
struct Uniforms {
    cell_size: vec2<f32>,
    viewport_size: vec2<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var atlas_texture: texture_2d_array<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;

struct GlyphInstance {
    @location(0) pos: vec2<f32>,
    @location(1) viewport_offset: vec2<f32>,
    @location(2) uv_offset: vec2<f32>,
    @location(3) uv_size: vec2<f32>,
    @location(4) fg_color: vec4<f32>,
    @location(5) glyph_offset: vec2<f32>,
    @location(6) glyph_size: vec2<f32>,
    @location(7) page: u32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) fg_color: vec4<f32>,
    @location(2) @interpolate(flat) page: u32,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, instance: GlyphInstance) -> VertexOutput {
    let quad_pos = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0),
    );

    let p = quad_pos[vi];
    let pixel_pos = instance.pos * uniforms.cell_size + instance.viewport_offset
                    + instance.glyph_offset + p * instance.glyph_size;
    let ndc = pixel_pos / uniforms.viewport_size * 2.0 - 1.0;

    var out: VertexOutput;
    out.position = vec4(ndc.x, -ndc.y, 0.0, 1.0);
    out.uv = instance.uv_offset + p * instance.uv_size;
    out.fg_color = instance.fg_color;
    out.page = instance.page;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(atlas_texture, atlas_sampler, in.uv, i32(in.page)).r;
    return vec4(in.fg_color.rgb, in.fg_color.a * alpha);
}
"#;
