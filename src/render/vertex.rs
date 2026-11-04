use glam::{I16Vec2, IVec2, UVec2, Vec2, Vec2Swizzles, Vec3, Vec3Swizzles, Vec4};
use windows::Win32::Graphics::Direct3D12::{
    D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA, D3D12_INPUT_ELEMENT_DESC,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R32G32B32_FLOAT, DXGI_FORMAT_R32G32B32A32_FLOAT, DXGI_FORMAT_R32_UINT, DXGI_FORMAT_R32G32B32_UINT, DXGI_FORMAT_R16_UINT, DXGI_FORMAT_R16G16_SINT, DXGI_FORMAT_R16G16B16A16_UINT, DXGI_FORMAT_R16G16_SNORM};
use windows::core::PCSTR;

pub trait Vertex: bytemuck::Pod + bytemuck::Zeroable + Copy {
    const LAYOUT: &'static [D3D12_INPUT_ELEMENT_DESC];
}

pub trait LodVertex: Vertex {
    fn pos(&self) -> Vec3;
}

pub trait VertexAttr {
    const FORMAT: DXGI_FORMAT;
}
impl VertexAttr for u32 {
    const FORMAT: DXGI_FORMAT = DXGI_FORMAT_R32_UINT;
}
impl VertexAttr for [u16; 4] {
    const FORMAT: DXGI_FORMAT = DXGI_FORMAT_R16G16B16A16_UINT;
}
impl VertexAttr for u16 {
    const FORMAT: DXGI_FORMAT = DXGI_FORMAT_R16_UINT;
}
impl VertexAttr for [i16; 2] {
    const FORMAT: DXGI_FORMAT = DXGI_FORMAT_R16G16_SNORM;
}
impl VertexAttr for [f32; 2] {
    const FORMAT: DXGI_FORMAT = DXGI_FORMAT_R32G32_FLOAT;
}
impl VertexAttr for [f32; 3] {
    const FORMAT: DXGI_FORMAT = DXGI_FORMAT_R32G32B32_FLOAT;
}
impl VertexAttr for [f32; 4] {
    const FORMAT: DXGI_FORMAT = DXGI_FORMAT_R32G32B32A32_FLOAT;
}

pub trait FromVertexProps {
    fn from_vertex_props(
        pos: Vec3,
        normal: Vec3,
        tangent: Vec3,
        binormal: Vec3,
        uv: Vec2,
        color: Vec4,
    ) -> Self;
}

pub const fn attr(name: &str, format: DXGI_FORMAT, offset: usize) -> D3D12_INPUT_ELEMENT_DESC {
    assert!(
        name.as_bytes()[name.len() - 1] == 0,
        "Semantic name must be null terminated"
    );
    D3D12_INPUT_ELEMENT_DESC {
        SemanticName: PCSTR(name.as_ptr() as _),
        SemanticIndex: 0,
        Format: format,
        InputSlot: 0,
        AlignedByteOffset: offset as u32,
        InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
        InstanceDataStepRate: 0,
    }
}

#[macro_export]
macro_rules! vertex_struct {
    ($name:ident { $($field:ident : $ty:ty as $sem:ident);* $(;)? }) => {
        #[repr(C)]
        #[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
        pub struct $name { $(pub $field: $ty),* }

        impl $name {
            #[allow(unused)]
            pub fn new($($field: impl Into<$ty>),*) -> Self {
                Self { $($field: $field.into()),* }
            }
        }

        impl $crate::render::vertex::Vertex for $name {
            const LAYOUT: &'static [windows::Win32::Graphics::Direct3D12::D3D12_INPUT_ELEMENT_DESC] = &[
                $($crate::render::vertex::attr(core::concat!(core::stringify!($sem), "\0"), <$ty as crate::render::vertex::VertexAttr>::FORMAT, core::mem::offset_of!($name, $field))),*
            ];
        }
    };
}

mod tests {
    vertex_struct!(
        VertPosColor {
            pos: [f32; 3] as POSITION;
            col: [f32; 4] as COLOR;
        }
    );

    #[test]
    pub fn test_vert_creation() {
        use crate::render::vertex::Vertex;
        use glam::{Vec3, Vec4};
        use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R32G32B32A32_FLOAT;

        let v = VertPosColor::new(Vec3::new(0., 3., 0.), Vec4::new(0.7, 0.3, 0.3, 1.));
        assert_eq!(size_of_val(&v), 28);
        assert_eq!(VertPosColor::LAYOUT[1].AlignedByteOffset, 12);
        assert_eq!(
            VertPosColor::LAYOUT[1].Format,
            DXGI_FORMAT_R32G32B32A32_FLOAT
        );
    }
}

// https://knarkowicz.wordpress.com/2014/04/16/octahedron-normal-vector-encoding/
pub fn octahedral_encode_normal(normal: Vec3) -> I16Vec2 {
    let mut v = normal / (normal.x.abs() + normal.y.abs() + normal.z.abs());
    if v.z < 0. {
        let oct_wrap = oct_wrap(v.xy());
        v.x = oct_wrap.x;
        v.y = oct_wrap.y;
    }

    let v = v.xy();
    let n = (v * i16::MAX as f32).round();
    I16Vec2::new(n.x as i16, n.y as i16)
}

fn oct_wrap(v: Vec2) -> Vec2 {
    let sign = Vec2::new(
        if v.x >= 0.0 { 1.0 } else { -1.0 },
        if v.y >= 0.0 { 1.0 } else { -1.0 },
    );
    (1.0 - v.yx().abs()) * sign
}