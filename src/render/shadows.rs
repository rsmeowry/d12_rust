use crate::render::builtin::Vertex3;
use crate::render::camera::Camera;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::res::{ResId, ResSize};
use crate::render::graph::FrameGraph;
use crate::render::lights::{GpuLight, LIGHT_POINT, LIGHT_SPOT};
use crate::render::mesh::Mesh;
use crate::render::perf::frustum::Frustum;
use crate::render::pipeline::desc::{Depth, PipelineDesc};
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{ObjectData, Renderer};
use crate::render::vertex::Vertex;
use crate::wrappers::graphics::GraphicsDevice;
use glam::{FloatExt, Mat4, Vec3, Vec4};
use std::cell::Cell;
use std::f32::consts::FRAC_PI_2;
use windows::Win32::Graphics::Direct3D12::{D3D12_COMPARISON_FUNC_GREATER_EQUAL, D3D12_CULL_MODE_FRONT, D3D12_CULL_MODE_NONE};

pub const CASCADE_COUNT: usize = 4;
pub const CAM_NEAR: f32 = 0.1;
pub const SHADOW_DISTANCE: f32 = 35.;
pub const SHADOW_EXTEND: f32 = 25.;
pub const LOCAL_NEAR: f32 = CAM_NEAR / 2.;
pub const CUBE_FACES: [(Vec3, Vec3); 6] = [
    (Vec3::X, Vec3::Y),
    (Vec3::NEG_X, Vec3::Y),
    (Vec3::Y, Vec3::NEG_Z),
    (Vec3::NEG_Y, Vec3::Z),
    (Vec3::Z, Vec3::Y),
    (Vec3::NEG_Z, Vec3::Y),
];
pub const MAX_SPOT_SHADOWS: usize = 8;
pub const MAX_POINT_SHADOWS: usize = 8;

#[derive(Debug)]
pub struct ShadowMap {
    pipeline: PipelineId,
    cascades: [ResId; CASCADE_COUNT],
    last_vp: [(Mat4, f32); CASCADE_COUNT],
    last_drawn: Cell<[usize; CASCADE_COUNT]>,
    size: u32,
    frame_idx: u32,
    local_shadows: LocalShadows
}

#[derive(Debug)]
pub struct LocalShadows {
    pipeline: PipelineId,
    spots: [ResId; MAX_SPOT_SHADOWS],
    points: [ResId; MAX_SPOT_SHADOWS],
    spot_size: u32,
    point_size: u32,
    last_spot: usize,
    last_point: usize,
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
    pub fn new(renderer: &mut Renderer, size: u32, spot_size: u32, point_size: u32) -> anyhow::Result<Self> {
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
            cull: D3D12_CULL_MODE_FRONT,
            depth_bias: -20,
            slope_bias: -1.0,
            depth_clip: false,
            ..Default::default()
        })?;

        let mut spots = [ResId::default(); MAX_SPOT_SHADOWS];
        for i in 0..MAX_SPOT_SHADOWS {
            spots[i] = renderer.gr.depth(&renderer.gd, &format!("Shadows_Spot{i}"), ResSize::Fixed(spot_size, spot_size))?;
        }
        let mut points = [ResId::default(); MAX_POINT_SHADOWS];
        for i in 0..MAX_POINT_SHADOWS {
            points[i] = renderer.gr.depth_cube(&renderer.gd, &format!("Shadows_Point{i}"), point_size)?;
        }

        Ok(Self {
            pipeline,
            cascades,
            last_vp: [(Mat4::IDENTITY, 0.); CASCADE_COUNT],
            frame_idx: 0,
            last_drawn: Cell::new([0; CASCADE_COUNT]),
            size,
            local_shadows: LocalShadows { pipeline, spots, spot_size, last_spot: 0, point_size, points, last_point: 0 },
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

    pub fn fetch_all_local_shadows(&self) -> Vec<ResId> {
        self.local_shadows.spots.iter().copied().chain(self.local_shadows.points.iter().copied()).collect::<Vec<ResId>>()
    }

    pub fn update(&mut self, main_light: GpuLight, lambda: f32, renderer: &Renderer) {
        // incrementing first so add_global_shadows_pass sees the same frame_idx we decided on
        self.frame_idx += 1;
        let splits = Self::calc_splits(CAM_NEAR, SHADOW_DISTANCE, lambda);
        for i in 0..CASCADE_COUNT {
            // checking if we should even update this cascade
            if self.frame_idx % (i + 1) as u32 != 0 {
                continue;
            }
            let (vp, texel) = Self::cascade_matrix(&renderer.cam, main_light.direction, splits[i], splits[i + 1], self.size);
            self.last_vp[i] = (vp, texel);
        }
    }

    pub fn add_global_shadows_pass<'f>(&self, lambda: f32, renderer: &'f Renderer, graph: &mut FrameGraph<'f>, objects: &[ObjectData], meshes: &[&'f Mesh]) -> anyhow::Result<ShadowCascades> {
        let objects_va = renderer.upload_objects(&objects)?;
        let splits = Self::calc_splits(CAM_NEAR, SHADOW_DISTANCE, lambda);

        let mut shadows = ShadowCascades {
            cascades: self.cascades,
            view_proj: [Mat4::IDENTITY; CASCADE_COUNT],
            splits: Vec4::new(splits[1], splits[2], splits[3], splits[4]),
            texel_sizes: Default::default(),
            drawn: [0; CASCADE_COUNT],
        };

        let pass_names = ["ShadowPass0", "ShadowPass1", "ShadowPass2", "ShadowPass3"];
        for i in 0..CASCADE_COUNT {
            let (vp, texel) = self.last_vp[i];
            shadows.view_proj[i] = vp;
            shadows.texel_sizes[i] = texel;

            if self.frame_idx % (i + 1) as u32 != 0 {
                shadows.drawn[i] = self.last_drawn.get()[i];
                continue;
            }

            shadows.drawn[i] =
                shadows_depth_pass(
                    graph, renderer, self.pipeline,
                    pass_names[i], self.cascades[i], 0,
                    vp, objects_va, objects, meshes)?;
        }
        self.last_drawn.set(shadows.drawn);


        Ok(shadows)
    }

    pub fn add_local_shadows_pass<'f>(&mut self, lights: &mut Vec<GpuLight>, renderer: &'f Renderer, objects_va: u64, objects: &[ObjectData], meshes: &[&'f Mesh], graph: &mut FrameGraph<'f>) -> anyhow::Result<()> {
        self.local_shadows.add_pass(lights, renderer, objects_va, objects, meshes, graph)
    }
}

impl LocalShadows {
    fn add_pass<'f>(&mut self, lights: &mut Vec<GpuLight>, renderer: &'f Renderer, objects_va: u64, objects: &[ObjectData], meshes: &[&'f Mesh], graph: &mut FrameGraph<'f>) -> anyhow::Result<()> {
        self.last_spot = 0;
        self.last_point = 0;
        for light in lights.iter_mut() {
            match light.kind {
                LIGHT_SPOT => {
                    if self.last_spot + 1 >= MAX_SPOT_SHADOWS {
                        continue; // exhausted our texture limit :(
                    }
                    let (vp, params) = Self::prepare_spot_light(light, self.spot_size);
                    let tex = self.spots[self.last_spot];
                    light.shadow_tex = renderer.gr.get(tex)?.srv_ptr();
                    shadows_depth_pass(
                        graph, renderer, self.pipeline,
                        "SpotLight", tex, 0, vp,
                        objects_va, objects, meshes)?;
                    self.last_spot += 1;
                },
                LIGHT_POINT => {
                    if self.last_point + 1 >= MAX_POINT_SHADOWS {
                        continue;
                    }
                    let (faces, params) = Self::prepare_point_light(light, self.point_size);
                    let tex = self.points[self.last_point];
                    for (face_idx, face) in faces.into_iter().enumerate() {
                        shadows_depth_pass(
                            graph, renderer, self.pipeline,
                            "PointLight",
                            tex, face_idx as u32, face,
                            objects_va, objects, meshes)?;
                    }
                    light.shadow_tex = renderer.gr.get(tex)?.srv_ptr();

                    self.last_point += 1;
                }
                _ => { }
            }
        }
        Ok(())
    }

    pub fn prepare_spot_light(light: &mut GpuLight, size: u32) -> (Mat4, Vec3) {
        let outer = light.spot_outer_cos.acos();
        let up = if light.direction.y.abs() < 0.99 { Vec3::Y } else { Vec3::X };
        let proj = glam::camera::lh::proj::directx::perspective(2.0 * outer, 1.0, light.range, LOCAL_NEAR);
        light.shadow_vp = proj * glam::camera::lh::view::look_to_mat4(light.position, light.direction, up);
        light.shadow_params = Vec3::new(0., 0., 2.0 * outer.tan() / size as f32);
        (light.shadow_vp, light.shadow_params)
    }

    pub fn prepare_point_light(light: &mut GpuLight, size: u32) -> ([Mat4; 6], Vec3) {
        let proj = glam::camera::lh::proj::directx::perspective(FRAC_PI_2, 1.0, light.range, LOCAL_NEAR);
        let mut vps = [Mat4::IDENTITY; 6];
        for i in 0..6 {
            vps[i] = proj * glam::camera::lh::view::look_to_mat4(light.position, CUBE_FACES[i].0, CUBE_FACES[i].1);
        }
        light.shadow_params = Vec3::new(proj.z_axis.z, proj.w_axis.z, 2.0 / size as f32);
        (vps, light.shadow_params)
    }
}

fn shadows_depth_pass<'f>(graph: &mut FrameGraph<'f>, renderer: &'f Renderer, pso: PipelineId,
                  name: &'static str, target: ResId, slice: u32, vp: Mat4, objects_va: u64,
                  objects: &[ObjectData], meshes: &[&'f Mesh]) -> anyhow::Result<usize> {
    let frustum = Frustum::from_view_proj(vp);
    let draws: Vec<(&'f Mesh, u32)> = meshes.iter().zip(objects).enumerate()
        .filter(|(_, (m, o))| frustum.contains(m.aabb(), o.model))
        .map(|(idx, (m, _))| (*m, idx as u32))
        .collect();
    let vp_va = renderer.upload_cbuffer(&vp)?;
    let (rs, pipelines, n) = (renderer.rs(), renderer.pipelines(), draws.len());
    graph.add_pass(name)
        .writes_depth_slice(target, ResLoad::Clear, slice)
        .exec(move |ctx| {
            rs.set_pass(ctx.cmd, vp_va);
            rs.set_objects(ctx.cmd, objects_va);
            pipelines.get(pso).bind(ctx.cmd);
            for (mesh, idx) in &draws {
                rs.set_draw_index(ctx.cmd, *idx);
                mesh.draw(ctx.cmd);
            }
            Ok(())
        });
    Ok(n)
}
