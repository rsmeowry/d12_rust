use glam::{Vec2, Vec3, Vec4};
use crate::render::perf::frustum::AABB;
use crate::render::vertex::{FromVertexProps, LodVertex};
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

vertex_struct!(VertexPbr {
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
    fn from_vertex_props(pos: Vec3, normal: Vec3, _tangent: Vec3, _binormal: Vec3, uv: Vec2, _color: Vec4) -> Self {
        Self::new(pos, normal, uv)
    }
}

impl FromVertexProps for VertexTess {
    fn from_vertex_props(pos: Vec3, normal: Vec3, tangent: Vec3, binormal: Vec3, uv: Vec2, _color: Vec4) -> Self {
        Self::new(pos, normal, tangent, binormal, uv)
    }
}

impl FromVertexProps for VertexPbr {
    fn from_vertex_props(pos: Vec3, normal: Vec3, tangent: Vec3, binormal: Vec3, uv: Vec2, _color: Vec4) -> Self {
        Self::new(pos, normal, tangent, binormal, uv)
    }
}

impl LodVertex for Vertex3 {
    fn pos(&self) -> Vec3 {
        return self.pos.into();
    }

    fn add(&mut self, other: &Self) {
        self.pos[0] += other.pos[0];
        self.pos[1] += other.pos[1];
        self.pos[2] += other.pos[2];
        
        self.uv[0] += other.uv[0];
        self.uv[1] += other.uv[1];
        
        self.norm[0] += other.norm[0];
        self.norm[1] += other.norm[1];
        self.norm[2] += other.norm[2];
    }

    fn div(&mut self, v: f32) {
        self.pos[0] /= v;
        self.pos[1] /= v;
        self.pos[2] /= v;
        
        self.uv[0] /= v;
        self.uv[1] /= v;

        self.norm[0] /= v;
        self.norm[1] /= v;
        self.norm[2] /= v;
    }
}

impl LodVertex for VertexPbr {
    fn pos(&self) -> Vec3 {
        return self.pos.into()
    }

    fn add(&mut self, other: &Self) {
        self.pos[0] += other.pos[0];
        self.pos[1] += other.pos[1];
        self.pos[2] += other.pos[2];

        self.tangent[0] += other.tangent[0];
        self.tangent[1] += other.tangent[1];
        self.tangent[2] += other.tangent[2];

        self.uv[0] += other.uv[0];
        self.uv[1] += other.uv[1];

        self.binormal[0] += other.binormal[0];
        self.binormal[1] += other.binormal[1];
        self.binormal[2] += other.binormal[2];

        self.norm[0] += other.norm[0];
        self.norm[1] += other.norm[1];
        self.norm[2] += other.norm[2];
    }

    fn div(&mut self, v: f32) {
        self.pos[0] /= v;
        self.pos[1] /= v;
        self.pos[2] /= v;

        self.tangent[0] /= v;
        self.tangent[1] /= v;
        self.tangent[2] /= v;

        self.uv[0] /= v;
        self.uv[1] /= v;

        self.binormal[0] /= v;
        self.binormal[1] /= v;
        self.binormal[2] /= v;

        self.norm[0] /= v;
        self.norm[1] /= v;
        self.norm[2] /= v;
    }
}

impl LodVertex for VertexTess {
    fn pos(&self) -> Vec3 {
        return self.pos.into()
    }

    fn add(&mut self, other: &Self) {
        self.pos[0] += other.pos[0];
        self.pos[1] += other.pos[1];
        self.pos[2] += other.pos[2];
        
        self.tangent[0] += other.tangent[0];
        self.tangent[1] += other.tangent[1];
        self.tangent[2] += other.tangent[2];
        
        self.uv[0] += other.uv[0];
        self.uv[1] += other.uv[1];
        
        self.binormal[0] += other.binormal[0];
        self.binormal[1] += other.binormal[1];
        self.binormal[2] += other.binormal[2];
        
        self.norm[0] += other.norm[0];
        self.norm[1] += other.norm[1];
        self.norm[2] += other.norm[2];
    }

    fn div(&mut self, v: f32) {
        self.pos[0] /= v;
        self.pos[1] /= v;
        self.pos[2] /= v;

        self.tangent[0] /= v;
        self.tangent[1] /= v;
        self.tangent[2] /= v;

        self.uv[0] /= v;
        self.uv[1] /= v;

        self.binormal[0] /= v;
        self.binormal[1] /= v;
        self.binormal[2] /= v;

        self.norm[0] /= v;
        self.norm[1] /= v;
        self.norm[2] /= v;
    }
}