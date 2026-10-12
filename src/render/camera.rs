use glam::{Mat4, Quat, Vec3};
use crate::render::perf::frustum::Frustum;

#[derive(Debug)]
pub struct Camera {
    fov_rad: f32,
    aspect: f32,
    pos: Vec3,
    #[allow(unused)]
    rot: Quat,

    view: Mat4,
    proj: Mat4,
    view_proj: Mat4,
    frustum: Frustum
}

#[allow(unused)]
impl Camera {
    pub fn new(width: u32, height: u32, fov_deg: f32, pos: Vec3, rot: Quat) -> Self {
        let fov_rad = fov_deg.to_radians();
        let aspect = width as f32 / height as f32;
        let view = glam::camera::lh::view::look_to_mat4(pos, rot.mul_vec3(Vec3::Z), Vec3::Y);
        let proj = Self::projection(aspect, fov_rad);
        let view_proj = proj * view;
        Self {
            fov_rad,
            aspect,
            pos,
            rot,
            view,
            proj,
            view_proj,
            frustum: Frustum::from_view_proj(view_proj)
        }
    }

    pub fn view_proj(&self) -> Mat4 {
        self.view_proj
    }

    pub fn frustum(&self) -> Frustum { self.frustum }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.aspect = width as f32 / height as f32;
        self.proj = Self::projection(self.aspect, self.fov_rad);
        self.view_proj = self.proj * self.view;
        self.frustum = Frustum::from_view_proj(self.view_proj);
    }

    pub fn set_fov(&mut self, fov_deg: f32) {
        self.fov_rad = fov_deg.to_radians();
        self.proj = Self::projection(self.aspect, self.fov_rad);
        self.view_proj = self.proj * self.view;
        self.frustum = Frustum::from_view_proj(self.view_proj);
    }

    pub fn set_pos(&mut self, pos: Vec3) {
        self.pos = pos;
        self.view =
            glam::camera::lh::view::look_to_mat4(self.pos, self.rot.mul_vec3(Vec3::Z), Vec3::Y);
        self.view_proj = self.proj * self.view;
        self.frustum = Frustum::from_view_proj(self.view_proj);
    }

    pub fn set_rot(&mut self, rot: Quat) {
        self.rot = rot;
        self.view =
            glam::camera::lh::view::look_to_mat4(self.pos, self.rot.mul_vec3(Vec3::Z), Vec3::Y);
        self.view_proj = self.proj * self.view;
        self.frustum = Frustum::from_view_proj(self.view_proj);
    }

    pub fn pos(&self) -> Vec3 {
        self.pos
    }

    pub fn rot(&self) -> Quat {
        self.rot
    }

    pub fn forward(&self) -> Vec3 {
        self.rot * Vec3::Z
    }

    pub fn view(&self) -> Mat4 {
        self.view
    }

    pub fn fov_rad(&self) -> f32 {
        self.fov_rad
    }

    pub fn aspect(&self) -> f32 {
        self.aspect
    }

    fn projection(aspect: f32, fov_rad: f32) -> Mat4 {
        glam::camera::lh::proj::directx::perspective(fov_rad, aspect, 10e11, 0.1)
    }
}
