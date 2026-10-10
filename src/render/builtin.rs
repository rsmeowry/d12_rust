use glam::{Vec2, Vec3, Vec4};
use crate::render::vertex::FromVertexProps;
use crate::vertex_struct;

vertex_struct!(Vertex3 {
    pos: [f32; 3] as POSITION;
    norm: [f32; 3] as NORMAL;
    uv: [f32; 2] as TEXCOORD;
});

vertex_struct!(VertexTess {
    pos: [f32; 3] as POSITION;
    norm: [f32; 3] as NORMAL;
    tangent: [f32; 3] as TANGENT;
    binormal: [f32; 3] as BINORMAL;
    uv: [f32; 2] as TEXCOORD;
});

impl FromVertexProps for Vertex3 {
    fn from_vertex_props(pos: Vec3, normal: Vec3, tangent: Vec3, binormal: Vec3, uv: Vec2, color: Vec4) -> Self {
        Self::new(pos, normal, uv)
    }
}

impl FromVertexProps for VertexTess {
    fn from_vertex_props(pos: Vec3, normal: Vec3, tangent: Vec3, binormal: Vec3, uv: Vec2, color: Vec4) -> Self {
        Self::new(pos, normal, tangent, binormal, uv)
    }
}