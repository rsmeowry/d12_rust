use glam::{Mat4, Quat, Vec3};

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
}

#[allow(unused)]
impl Camera {
    pub fn new(width: u32, height: u32, fov_deg: f32, pos: Vec3, rot: Quat) -> Self {
        let fov_rad = fov_deg.to_radians();
        let aspect = width as f32 / height as f32;
        let view = glam::camera::lh::view::look_to_mat4(pos, rot.mul_vec3(Vec3::Z), Vec3::Y);
        let proj = glam::camera::lh::proj::directx::perspective(fov_rad, aspect, 0.1, 100.);
        Self {
            fov_rad,
            aspect,
            pos,
            rot,
            view,
            proj,
            view_proj: proj * view
        }
    }

    pub fn view_proj(&self) -> Mat4 {
        self.view_proj
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.aspect = width as f32 / height as f32;
        self.proj = glam::camera::lh::proj::directx::perspective(self.fov_rad, self.aspect, 0.1, 100.);
        self.view_proj = self.proj * self.view;
    }

    pub fn set_fov(&mut self, fov_deg: f32) {
        self.fov_rad = fov_deg.to_radians();
        self.proj = glam::camera::lh::proj::directx::perspective(self.fov_rad, self.aspect, 0.1, 100.);
        self.view_proj = self.proj * self.view;
    }

    pub fn set_pos(&mut self, pos: Vec3) {
        self.pos = pos;
        self.view = glam::camera::lh::view::look_to_mat4(self.pos, self.rot.mul_vec3(Vec3::NEG_Z), Vec3::Y);
        self.view_proj = self.proj * self.view;
    }

    pub fn set_rot(&mut self, rot: Quat) {
        self.rot = rot;
        self.view = glam::camera::lh::view::look_to_mat4(self.pos, self.rot.mul_vec3(Vec3::NEG_Z), Vec3::Y);
        self.view_proj = self.proj * self.view;
    }

    pub fn pos(&self) -> Vec3 {
        self.pos
    }

    pub fn rot(&self) -> Quat {
        self.rot
    }
}