use glam::{Vec2, Vec3, Vec4};
use crate::render::perf::frustum::AABB;
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

pub fn aabb_from_vertices3(vertices: &[Vertex3]) -> AABB {
    AABB::from_points(&vertices.iter().map(|it| Vec3::from(it.pos)).collect::<Vec<_>>())
}

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