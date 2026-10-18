use crate::render::builtin::Vertex3;
use crate::render::camera::Camera;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::res::{GraphResources, ResId, ResSize};
use crate::render::graph::FrameGraph;
use crate::render::lights::{GpuLight, LIGHT_POINT, LIGHT_SPOT};
use crate::render::mesh::Mesh;
use crate::render::perf::frustum::Frustum;
use crate::render::pipeline::desc::{Depth, PipelineDesc};
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{ObjectData, Renderer};
use crate::render::vertex::Vertex;
use crate::wrappers::graphics::GraphicsDevice;
use bytemuck::Zeroable;
use glam::{FloatExt, Mat4, Vec3, Vec4};
use std::cell::Cell;
use std::f32::consts::FRAC_PI_2;
use windows::Win32::Graphics::Direct3D12::{D3D12_COMPARISON_FUNC_GREATER_EQUAL, D3D12_CULL_MODE_FRONT};

pub const CASCADE_COUNT: usize = 4;
pub const CAM_NEAR: f32 = 0.1;
pub const SHADOW_DISTANCE: f32 = 35.;
pub const SHADOW_EXTEND: f32 = 25.;
pub const STATIC_MARGIN: f32 = 1.5;
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

// https://dev.epicgames.com/documentation/unreal-engine/movable-light-mobility-in-unreal-engine
// static caching logic from here but applied to cascades
#[derive(Debug)]
pub struct ShadowMap {
    pipeline: PipelineId,
    cascades: [ResId; CASCADE_COUNT],
    cascades_static: [ResId; CASCADE_COUNT],
    dynamic_vp: [(Mat4, f32); CASCADE_COUNT],
    statics: [StaticCascade; CASCADE_COUNT],
    static_light_dir: Vec3,
    static_size: u32,
    dynamic_size: u32,
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
    pub cascades_static: [ResId; CASCADE_COUNT],
    pub view_proj: [Mat4; CASCADE_COUNT],
    pub static_view_proj: [Mat4; CASCADE_COUNT],
    pub splits: Vec4, // distances, just assuming we have 4 cascades (🙏🙏)
    pub texel_sizes: Vec4, // texel sizes for each cascade
    pub static_texel_sizes: Vec4,
    pub drawn: [usize; CASCADE_COUNT], // how many objects per cascade
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuCascades {
    pub vp: [Mat4; CASCADE_COUNT],
    pub static_vp: [Mat4; CASCADE_COUNT],
    pub splits: Vec4,
    pub texel: Vec4,
    pub texel_static: Vec4,
    pub tex: [u32; CASCADE_COUNT],
    pub tex_static: [u32; CASCADE_COUNT],
    pub cam_forward: Vec3,
    pub enabled: u32,
}

impl GpuCascades {
    pub fn new(shadows: Option<&ShadowCascades>, gr: &GraphResources, cam_forward: Vec3) -> anyhow::Result<Self> {
        let Some(s) = shadows else {
            return Ok(Self { vp: [Mat4::IDENTITY; CASCADE_COUNT], cam_forward, ..Self::zeroed() });
        };
        let mut tex = [0u32; CASCADE_COUNT];
        for (dst, id) in tex.iter_mut().zip(s.cascades) {
            *dst = gr.get(id)?.srv_ptr();
        }
        let mut tex_static = [0u32; CASCADE_COUNT];
        for (dst, id) in tex_static.iter_mut().zip(s.cascades_static) {
            *dst = gr.get(id)?.srv_ptr();
        }
        Ok(Self { vp: s.view_proj, static_vp: s.static_view_proj, splits: s.splits, texel: s.texel_sizes, texel_static: s.static_texel_sizes, tex, tex_static, cam_forward, enabled: 1 })
    }
}

#[derive(Debug, Copy, Clone)]
struct StaticCascade {
    vp: Mat4,
    texel: f32,
    center_ls: Vec3, // texel-snapped
    cover: f32,
    valid: bool,
    dirty: bool,
}

impl ShadowMap {
    pub fn new(renderer: &mut Renderer, dynamic_size: u32, static_size: u32, spot_size: u32, point_size: u32) -> anyhow::Result<Self> {
        let mut cascades = [ResId::default(); CASCADE_COUNT];
        let mut cascades_static = [ResId::default(); CASCADE_COUNT];
        for i in 0..CASCADE_COUNT {
            cascades[i] = renderer.gr.depth(&renderer.gd, &format!("Shadows{i}"), ResSize::Fixed(dynamic_size, dynamic_size))?;
            cascades_static[i] = renderer.gr.depth(&renderer.gd, &format!("ShadowsStatic{i}"), ResSize::Fixed(static_size, static_size))?;
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
            cascades_static,
            dynamic_vp: [(Mat4::IDENTITY, 0.); CASCADE_COUNT],
            statics: [StaticCascade {
                vp: Mat4::IDENTITY,
                texel: 0.0,
                center_ls: Default::default(),
                cover: 0.0,
                valid: false,
                dirty: true,
            }; CASCADE_COUNT],
            dynamic_size,
            static_size,
            local_shadows: LocalShadows { pipeline, spots, spot_size, last_spot: 0, point_size, points, last_point: 0 },
            static_light_dir: Default::default(),
        })
    }

    pub fn invalidate_static(&mut self) {
        for i in 0..CASCADE_COUNT {
            self.statics[i].valid = false;
        }
    }

    fn calc_splits(n: f32, f: f32, lambda: f32) -> [f32; CASCADE_COUNT + 1] {
        std::array::from_fn(|i| {
            let t = i as f32 / CASCADE_COUNT as f32;
            let log = n * (f / n).powf(t);
            let uni = n + (f - n) * t;
            log.lerp(uni, lambda)
        })
    }

    fn fit_sphere(cam: &Camera, near: f32, far: f32) -> (Vec3, f32) {
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
        let center = corners.iter().sum::<Vec3>() / 8.;
        let mut rad = corners.iter().map(|each| each.distance(center)).fold(0., f32::max);
        rad = (rad * 16.).ceil() / 16.; // quantizing cus floats can be noisy sometimes

        (center, rad)
    }

    fn build_cascade_matrix(dir: Vec3, center: Vec3, rad: f32, size: u32) -> (Mat4, f32, Vec3) {
        let texel = 2.0 * rad / size as f32;

        // calculating lightspace matrix
        let up = if dir.y.abs() < 0.99 { Vec3::Y } else { Vec3::X };
        let light_view = glam::camera::lh::view::look_to_mat4(Vec3::ZERO, dir, up);

        // snapping center to texels
        let mut center = light_view.transform_point3(center);
        center.x = (center.x / texel).floor() * texel;
        center.y = (center.y / texel).floor() * texel;

        // orthographic
        let proj = glam::camera::lh::proj::directx::orthographic(
            center.x - rad, center.x + rad,
            center.y - rad, center.y + rad,
            center.z + rad, center.z - rad - SHADOW_EXTEND
        );
        (proj * light_view, texel, center)
    }

    pub fn fetch_all_local_shadows(&self) -> Vec<ResId> {
        self.local_shadows.spots.iter().copied().chain(self.local_shadows.points.iter().copied()).collect::<Vec<ResId>>()
    }

    pub fn update(&mut self, main_light: GpuLight, lambda: f32, renderer: &Renderer) {
        for i in 0..CASCADE_COUNT {
            self.statics[i].dirty = false;
        }

        // checking if we should rebake static shadowmaps
        if self.static_light_dir.dot(main_light.direction) < 0.9999 {
            self.static_light_dir = main_light.direction;
            self.invalidate_static();
        }

        let splits = Self::calc_splits(CAM_NEAR, SHADOW_DISTANCE, lambda);
        for i in 0..CASCADE_COUNT {
            // for dynamics
            let (center, rad) = Self::fit_sphere(&renderer.cam, splits[i], splits[i + 1]);
            let (vp, texel, _) = Self::build_cascade_matrix(main_light.direction, center, rad, self.dynamic_size);

            self.dynamic_vp[i] = (vp, texel);

            // for statics
            let up = if main_light.direction.y.abs() < 0.99 { Vec3::Y } else { Vec3::X };
            let light_view = glam::camera::lh::view::look_to_mat4(Vec3::ZERO, main_light.direction, up);
            let center_ls = light_view.transform_point3(center);
            let dist = (center_ls - self.statics[i].center_ls).abs();
            let static_fits = self.statics[i].valid &&
                dist.x + rad <= self.statics[i].cover &&
                dist.y + rad <= self.statics[i].cover &&
                dist.z + rad <= self.statics[i].cover;

            if !static_fits {
                self.statics[i].cover = (rad * STATIC_MARGIN * 16.).ceil() / 16.;

                let (static_vp, texel, center_ls) = Self::build_cascade_matrix(main_light.direction, center, self.statics[i].cover, self.static_size);

                self.statics[i].vp = static_vp;
                self.statics[i].texel = texel;
                self.statics[i].center_ls = center_ls;
                self.statics[i].valid = true;
                self.statics[i].dirty = true;
            }
        }
    }

    pub fn add_global_shadows_pass<'f>(&self, lambda: f32, renderer: &'f Renderer, graph: &mut FrameGraph<'f>, objects: &[ObjectData], meshes: &[&'f Mesh]) -> anyhow::Result<ShadowCascades> {
        let objects_va = renderer.upload_objects(&objects)?;
        let splits = Self::calc_splits(CAM_NEAR, SHADOW_DISTANCE, lambda);

        let mut shadows = ShadowCascades {
            cascades: self.cascades,
            cascades_static: self.cascades_static,
            static_view_proj: [Mat4::IDENTITY; CASCADE_COUNT],
            view_proj: [Mat4::IDENTITY; CASCADE_COUNT],
            splits: Vec4::new(splits[1], splits[2], splits[3], splits[4]),
            texel_sizes: Default::default(),
            static_texel_sizes: Default::default(),
            drawn: [0; CASCADE_COUNT],
        };

        let pass_names = ["ShadowPass0", "ShadowPass1", "ShadowPass2", "ShadowPass3"];
        let pass_names_static = ["StaticShadowPass0", "StaticShadowPass1", "StaticShadowPass2", "StaticShadowPass3"];
        for i in 0..CASCADE_COUNT {
            let (vp, texel) = self.dynamic_vp[i];
            shadows.view_proj[i] = vp;
            shadows.texel_sizes[i] = texel;
            shadows.static_texel_sizes[i] = self.statics[i].texel;
            shadows.static_view_proj[i] = self.statics[i].vp;

            // dynamic
            shadows.drawn[i] =
                shadows_depth_pass(
                    graph, renderer, self.pipeline,
                    pass_names[i], self.cascades[i], 0,
                    vp, objects_va, objects, meshes, StaticObjects::IncludeDynamic, ResLoad::Clear)?;

            if self.statics[i].dirty {
                shadows.drawn[i] += shadows_depth_pass(
                    graph, renderer, self.pipeline,
                    pass_names_static[i], self.cascades_static[i], 0,
                    self.statics[i].vp, objects_va, objects, meshes, StaticObjects::IncludeStatic, ResLoad::Clear)?;
            }
        }

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
                    let (vp, _params) = Self::prepare_spot_light(light, self.spot_size);
                    let tex = self.spots[self.last_spot];
                    light.shadow_tex = renderer.gr.get(tex)?.srv_ptr();
                    shadows_depth_pass(
                        graph, renderer, self.pipeline,
                        "SpotLight", tex, 0, vp,
                        objects_va, objects, meshes, StaticObjects::IncludeAll, ResLoad::Clear)?;
                    self.last_spot += 1;
                },
                LIGHT_POINT => {
                    if self.last_point + 1 >= MAX_POINT_SHADOWS {
                        continue;
                    }
                    let (faces, _params) = Self::prepare_point_light(light, self.point_size);
                    let tex = self.points[self.last_point];
                    for (face_idx, face) in faces.into_iter().enumerate() {
                        shadows_depth_pass(
                            graph, renderer, self.pipeline,
                            "PointLight",
                            tex, face_idx as u32, face,
                            objects_va, objects, meshes, StaticObjects::IncludeAll, ResLoad::Clear)?;
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
                  objects: &[ObjectData], meshes: &[&'f Mesh], statics: StaticObjects, load: ResLoad) -> anyhow::Result<usize> {
    let frustum = Frustum::from_view_proj(vp);
    let draws: Vec<(&'f Mesh, u32)> = meshes.iter().zip(objects).enumerate()
        .filter(|(_, (m, o))| statics.applies_to(m) && frustum.contains(m.aabb(), o.model))
        .map(|(idx, (m, _))| (*m, idx as u32))
        .collect();
    let vp_va = renderer.upload_cbuffer(&vp)?;
    let (rs, pipelines, n) = (renderer.rs(), renderer.pipelines(), draws.len());
    graph.add_pass(name)
        .writes_depth_slice(target, load, slice)
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum StaticObjects {
    IncludeAll,
    IncludeStatic,
    IncludeDynamic
}

impl StaticObjects {
    fn applies_to(&self, mesh: &Mesh) -> bool {
        match self {
            StaticObjects::IncludeAll => true,
            StaticObjects::IncludeStatic => mesh.is_static(),
            StaticObjects::IncludeDynamic => !mesh.is_static()
        }
    }
}