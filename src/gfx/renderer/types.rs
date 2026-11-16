use bytemuck::{Pod, Zeroable};
use tasty_type_appearance::color::GpuRgba;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub(crate) struct Uniforms {
    pub cell_size: [f32; 2],
    pub viewport_size: [f32; 2],
}

impl Uniforms {
    /// 셀 셰이더 uniform. renderer 생성과 resize가 같은 함수로 만든다.
    pub(crate) fn new(cell_size: [f32; 2], width: u32, height: u32) -> Self {
        Self {
            cell_size,
            viewport_size: [width as f32, height as f32],
        }
    }
}

/// renderer를 만들 때 셀 셰이더에 쓸 첫 viewport 크기.
/// configure한 surface 설정에서만 만들 수 있어 고정값으로 renderer를 만들 수 없다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportSize {
    width: u32,
    height: u32,
}

impl ViewportSize {
    pub fn of_surface(config: &wgpu::SurfaceConfiguration) -> Self {
        Self {
            width: config.width,
            height: config.height,
        }
    }

    pub(crate) fn uniforms(self, cell_size: [f32; 2]) -> Uniforms {
        Uniforms::new(cell_size, self.width, self.height)
    }
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

#[cfg(test)]
mod tests {
    use super::{Uniforms, ViewportSize};

    fn surface(width: u32, height: u32) -> wgpu::SurfaceConfiguration {
        wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8Unorm,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        }
    }

    #[test]
    fn uniforms_write_the_given_viewport_size_and_cell_size() {
        let u = Uniforms::new([9.0, 18.0], 2560, 1440);
        assert_eq!(u.viewport_size, [2560.0, 1440.0]);
        assert_eq!(u.cell_size, [9.0, 18.0]);
    }

    #[test]
    fn first_uniforms_use_the_configured_surface_size() {
        let u = ViewportSize::of_surface(&surface(2560, 1440)).uniforms([9.0, 18.0]);
        assert_eq!(u.viewport_size, [2560.0, 1440.0]);
        assert_eq!(u.cell_size, [9.0, 18.0]);
    }
}
