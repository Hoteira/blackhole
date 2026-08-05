use glam::{Mat4, Vec3};

pub struct Camera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_y: f32,
    pub aspect: f32,
    pub z_near: f32,
    pub z_far: f32,
}

impl Camera {
    pub fn new(target: Vec3, distance: f32, aspect: f32) -> Self {
        Self {
            target,
            distance,
            yaw: 0.0,
            pitch: 0.35,
            fov_y: std::f32::consts::FRAC_PI_4,
            aspect,
            z_near: 0.1,
            z_far: 1000.0,
        }
    }

    pub fn eye(&self) -> Vec3 {
        let pitch = self.pitch.clamp(
            -std::f32::consts::FRAC_PI_2 + 0.01,
            std::f32::consts::FRAC_PI_2 - 0.01,
        );

        self.target
            + Vec3::new(
                self.distance * pitch.cos() * self.yaw.sin(),
                self.distance * pitch.sin(),
                self.distance * pitch.cos() * self.yaw.cos(),
            )
    }

    pub fn view_projection(&self) -> Mat4 {
        let projection = glam::camera::rh::proj::directx::perspective(
            self.fov_y,
            self.aspect,
            self.z_near,
            self.z_far,
        );
        let view = glam::camera::rh::view::look_at_mat4(self.eye(), self.target, Vec3::Y);

        projection * view
    }
}
