use glam::{Vec2, Vec3, Vec4};
use windows::Win32::Graphics::Direct3D12::{
    D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA, D3D12_INPUT_ELEMENT_DESC,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R32G32B32_FLOAT,
    DXGI_FORMAT_R32G32B32A32_FLOAT,
};
use windows::core::PCSTR;

pub trait Vertex: bytemuck::Pod {
    const LAYOUT: &'static [D3D12_INPUT_ELEMENT_DESC];
}

pub trait VertexAttr {
    const FORMAT: DXGI_FORMAT;
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
