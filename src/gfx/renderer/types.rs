use bytemuck::{Pod, Zeroable};
use tasty_type_appearance::color::GpuRgba;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub(crate) struct Uniforms {
    pub cell_size: [f32; 2],
    pub viewport_size: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub(crate) struct BgInstance {
    pub pos: [f32; 2],
    pub viewport_offset: [f32; 2],
    pub bg_color: GpuRgba,
    /// 셀 위쪽에서 잰 (위쪽, 높이) 물리 픽셀. 높이가 0이면 칸 전체를 칠한다.
    pub line: [f32; 2],
}

impl BgInstance {
    pub(crate) const WHOLE_CELL: [f32; 2] = [0.0, 0.0];
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub(crate) struct GlyphInstance {
    pub pos: [f32; 2],
    pub viewport_offset: [f32; 2],
    pub uv_offset: [f32; 2],
    pub uv_size: [f32; 2],
    pub fg_color: GpuRgba,
    pub glyph_offset: [f32; 2],
    pub glyph_size: [f32; 2],
    /// Atlas page (D2Array layer) the glyph lives on.
    pub page: u32,
    /// Padding to keep the next instance 8-byte aligned.
    pub _pad: u32,
}
