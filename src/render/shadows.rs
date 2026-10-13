use glam::{FloatExt, Mat4, Vec3, Vec4};
use windows::Win32::Graphics::Direct3D12::{D3D12_COMPARISON_FUNC_GREATER_EQUAL, D3D12_COMPARISON_FUNC_LESS_EQUAL, D3D12_CULL_MODE_FRONT, D3D12_CULL_MODE_NONE};
use crate::render::builtin::Vertex3;
use crate::render::camera::Camera;
use crate::render::graph::FrameGraph;
use crate::render::graph::pass::{PassCtx, ResLoad};
use crate::render::graph::res::{ResId, ResSize};
use crate::render::lights::GpuLight;
use crate::render::mesh::Mesh;
use crate::render::perf::frustum::Frustum;
use crate::render::pipeline::desc::{Depth, PipelineDesc};
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{ObjectData, Renderer};
use crate::render::texture::Texture;
use crate::render::vertex::Vertex;
use crate::wrappers::graphics::GraphicsDevice;

pub const CASCADE_COUNT: usize = 4;
pub const CAM_NEAR: f32 = 0.1;
pub const SHADOW_DISTANCE: f32 = 35.;
pub const SHADOW_EXTEND: f32 = 25.;

 #[derive(Debug)]
pub struct ShadowMap {
    pipeline: PipelineId,
    cascades: [ResId; CASCADE_COUNT],
    size: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct ShadowCascades {
    pub cascades: [ResId; CASCADE_COUNT],
    pub view_proj: [Mat4; CASCADE_COUNT],
    pub splits: Vec4, // distances, just assuming we have 4 cascades (🙏🙏)
    pub texel_sizes: Vec4, // texel sizes for each cascade
    pub drawn: [usize; CASCADE_COUNT], // how many objects per cascade
}

impl ShadowMap {
    pub fn new(renderer: &mut Renderer,  size: u32) -> anyhow::Result<Self> {
        let mut cascades = [ResId::default(); CASCADE_COUNT];
        for i in 0..CASCADE_COUNT {
            cascades[i] = renderer.gr.depth(&renderer.gd, &format!("Shadows{i}"), ResSize::Fixed(size, size))?
        }
        let pipeline = renderer.create_pipeline(PipelineDesc {
            name: "ShadowDepth",
            shader: "deferred/ShadowDepth.hlsl".into(),
            input: Vertex3::LAYOUT,
            rts: vec![],
            depth: Some(Depth {
                format: GraphicsDevice::DS_FORMAT,
                func: D3D12_COMPARISON_FUNC_GREATER_EQUAL,
                write: true,
            }),
            cull: D3D12_CULL_MODE_NONE,
            depth_bias: -100,
            slope_bias: -2.0,
            depth_clip: false,
            ..Default::default()
        })?;
        Ok(Self {
            pipeline,
            cascades,
            size
        })
    }

    fn calc_splits(n: f32, f: f32, lambda: f32) -> [f32; CASCADE_COUNT + 1] {
        std::array::from_fn(|i| {
            let t = i as f32 / CASCADE_COUNT as f32;
            let log = n * (f / n).powf(t);
            let uni = n + (f - n) * t;
            log.lerp(uni, lambda)
        })
    }

    fn cascade_matrix(cam: &Camera, dir: Vec3, near: f32, far: f32, size: u32) -> (Mat4, f32) {
        // taking frustum edges
        let theta = (cam.fov_rad() * 0.5).tan();
        let tw = theta * cam.aspect();
        let inv_view = cam.view().inverse();
        let mut corners = [Vec3::ZERO; 8];
        let mut k = 0;
        for z in [near, far] {
            for sy in [-1., 1.] {
                for sx in [-1., 1.] {
                    corners[k] = inv_view.transform_point3(Vec3::new(sx * tw * z, sy * theta * z, z));
                    k += 1;
                }
            }
        }

        // taking a sphere around frustum
        let mut center = corners.iter().sum::<Vec3>() / 8.;
        let mut rad = corners.iter().map(|each| each.distance(center)).fold(0., f32::max);
        rad = (rad * 16.).ceil() / 16.; // quantizing cus floats can be noisy sometimes
        let texel = 2.0 * rad / size as f32;

        // calculating lightspace matrix
        let light_view = glam::camera::lh::view::look_to_mat4(Vec3::ZERO, dir, Vec3::X);

        // snapping center to texels
        center = light_view.transform_point3(center);
        center.x = (center.x / texel).floor() * texel;
        center.y = (center.y / texel).floor() * texel;

        // orthographic
        let proj = glam::camera::lh::proj::directx::orthographic(
            center.x - rad, center.x + rad,
            center.y - rad, center.y + rad,
            center.z + rad, center.z - rad - SHADOW_EXTEND
        );
        (proj * light_view, texel)
    }

    pub fn add_pass<'f>(&'f self, main_light: GpuLight, lambda: f32, renderer: &'f Renderer, graph: &mut FrameGraph<'f>, objects: Vec<ObjectData>, meshes: Vec<&'f Mesh>) -> anyhow::Result<ShadowCascades> {
        let objects_va = renderer.upload_objects(&objects)?;
        let splits = Self::calc_splits(CAM_NEAR, SHADOW_DISTANCE, lambda);
        let pipelines = renderer.pipelines();
        let rs = renderer.rs();

        let mut shadows = ShadowCascades {
            cascades: self.cascades,
            view_proj: [Mat4::IDENTITY; CASCADE_COUNT],
            splits: Vec4::new(splits[1], splits[2], splits[3], splits[4]),
            texel_sizes: Default::default(),
            drawn: [0; CASCADE_COUNT],
        };

        for i in 0..CASCADE_COUNT {
            let (vp, texel) = Self::cascade_matrix(&renderer.cam, main_light.direction, splits[i], splits[i + 1], self.size);
            shadows.view_proj[i] = vp;
            shadows.texel_sizes[i] = texel;

            let frustum = Frustum::from_view_proj(vp);
            let draws: Vec<(&'f Mesh, u32)> = meshes.iter().zip(&objects).enumerate()
                .filter(|(_, (m, o))| frustum.contains(m.aabb(), o.model))
                .map(|(idx, (m, _))| (*m, idx as u32))
                .collect();
            shadows.drawn[i] = draws.len();

            let lights_vp_va = renderer.upload_cbuffer(&vp)?;

            graph
                .add_pass("Shadow0")
                .writes_depth(self.cascades[i], ResLoad::Clear)
                .exec(move |ctx| {
                    rs.set_pass(ctx.cmd, lights_vp_va);
                    rs.set_objects(ctx.cmd, objects_va);
                    pipelines.get(self.pipeline).bind(ctx.cmd);
                    for (mesh, idx) in &draws {
                        rs.set_draw_index(ctx.cmd, *idx);
                        mesh.draw(ctx.cmd);
                    }
                    Ok(())
                });
        }

        Ok(shadows)
    }
}