use std::cmp::Ordering;
use glam::{Mat3, Mat4, Vec3, Vec4, Vec4Swizzles};
use crate::render::perf::octree::{BoxStatus, Octree};

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct AABB {
    pub min: Vec3,
    pub max: Vec3,
}

impl AABB {
    pub fn from_points(positions: &[Vec3]) -> Self {
        let (min, max) = positions.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), &p| (lo.min(p), hi.max(p)));
        Self {
            min,
            max
        }
    }

    pub fn contains_box(&self, o: &AABB) -> bool {
        self.min.cmple(o.min).all() && o.max.cmple(self.max).all()
    }

    pub fn transformed(&self, model: Mat4) -> Self {
        let center = model.transform_point3((self.min + self.max) * 0.5);
        let a = Mat3::from_mat4(model);
        let abs = Mat3::from_cols(a.x_axis.abs(), a.y_axis.abs(), a.z_axis.abs());
        let extends = abs * ((self.max - self.min) * 0.5);
        Self {
            min: center - extends,
            max: center + extends,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Frustum {
    top: Vec4,
    bottom: Vec4,
    right: Vec4,
    left: Vec4,
    far: Vec4,
    near: Vec4
}

// https://www.gamedevs.org/uploads/fast-extraction-viewing-frustum-planes-from-world-view-projection-matrix.pdf
impl Frustum {
    pub fn from_view_proj(view_proj: Mat4) -> Self {
        let m = view_proj.to_cols_array_2d();
        // left
        let left = Vec4::new(
            m[0][3] + m[0][0],
            m[1][3] + m[1][0],
            m[2][3] + m[2][0],
            m[3][3] + m[3][0],
        );
        let right = Vec4::new(
            m[0][3] - m[0][0],
            m[1][3] - m[1][0],
            m[2][3] - m[2][0],
            m[3][3] - m[3][0],
        );
        let bottom = Vec4::new(
            m[0][3] + m[0][1],
            m[1][3] + m[1][1],
            m[2][3] + m[2][1],
            m[3][3] + m[3][1],
        );
        let top = Vec4::new(
            m[0][3] - m[0][1],
            m[1][3] - m[1][1],
            m[2][3] - m[2][1],
            m[3][3] - m[3][1],
        );
        let near = Vec4::new(
            m[0][2],
            m[1][2],
            m[2][2],
            m[3][2]
        );
        let far = Vec4::new(
            m[0][3] - m[0][2],
            m[1][3] - m[1][2],
            m[2][3] - m[2][2],
            m[3][3] - m[3][2],
        );

        Self {
            near,
            far,
            top,
            bottom,
            left,
            right
        }
    }

    pub fn contains(&self, aabb: &AABB, model: Mat4) -> bool {
        self.classify(&aabb.transformed(model)) != BoxStatus::Outside
    }

    pub fn classify(&self, aabb: &AABB) -> BoxStatus {
        let mut result = BoxStatus::Inside;
        let planes = &[self.left, self.right, self.top, self.bottom, self.far, self.near];
        for plane in planes {
            let norm = plane.xyz();
            let pos = norm.cmpge(Vec3::ZERO);
            let p = Vec3::select(pos, aabb.max, aabb.min); // farthest along normal (inside)
            let q = Vec3::select(pos, aabb.min, aabb.max); // opposite

            if norm.dot(p) + plane.w < 0. {
                return BoxStatus::Outside; // even the inside one is outside lol
            }
            if norm.dot(q) + plane.w < 0. {
                result = BoxStatus::Intersects;
            }
        }
        return result;
    }
}

#[cfg(test)]
mod tests {
    use glam::{Mat4, Quat, Vec3};
    use crate::render::camera::Camera;
    use crate::render::perf::frustum::AABB;

    #[test]
    fn test_aabb() {
        let t = AABB { min: Vec3::new(-5., -0.1, -0.1), max: Vec3::new(5., 0.1, 0.1) }
            .transformed(Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2));
        assert!((t.max - Vec3::new(0.1, 0.1, 5.)).abs().max_element() < 1e-5);
    }

    #[test]
    fn test_simple_cull() {
        let cam = Camera::new(
            800, 600,
            60., Vec3::ZERO,
            Quat::from_rotation_y(0.));

        let aabb = AABB::from_points(&[
            Vec3::new(-1., -1., 0.),
            Vec3::new(1., 1., 1.)
        ]);
        let in_front = Mat4::from_translation(4. * Vec3::Z);
        let behind = Mat4::from_translation(-4. * Vec3::Z);

        assert!(cam.frustum().contains(&aabb, in_front));
        assert!(!cam.frustum().contains(&aabb, behind));
    }
}