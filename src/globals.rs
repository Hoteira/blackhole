use glam::{Mat4, Vec3};

pub const BLOOM_STRENGTH: f32 = 0.5;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Globals {
    pub view_proj: [[f32; 4]; 4],
    pub light_dir: [f32; 4],
    pub eye: [f32; 4],
    pub hole: [f32; 4],
    pub hud: [f32; 4],
    pub tonemap: [f32; 4],
    pub cam_right: [f32; 4],
    pub cam_up: [f32; 4],
    pub cam_fwd: [f32; 4],
}

impl Globals {
    pub fn new(
        view_proj: Mat4,
        light_dir: Vec3,
        eye: Vec3,
        hole: Vec3,
        r_s: f32,
        frame_ms: f32,
        aspect: f32,
        exposure: f32,
        srgb_surface: bool,
        basis: (Vec3, Vec3, Vec3),
        tan_half_fov: f32,
        raymarch: f32,
        spin: f32,
    ) -> Self {
        let d = light_dir.normalize_or_zero();

        Self {
            view_proj: view_proj.to_cols_array_2d(),
            light_dir: [d.x, d.y, d.z, 0.0],
            eye: [eye.x, eye.y, eye.z, 0.0],
            hole: [hole.x, hole.y, hole.z, r_s],
            hud: [frame_ms, aspect, crate::particles::C, spin],
            tonemap: [
                exposure,
                if srgb_surface { 1.0 } else { 0.0 },
                raymarch,
                BLOOM_STRENGTH,
            ],
            cam_right: [basis.0.x, basis.0.y, basis.0.z, 0.0],
            cam_up: [basis.1.x, basis.1.y, basis.1.z, 0.0],
            cam_fwd: [basis.2.x, basis.2.y, basis.2.z, tan_half_fov],
        }
    }

    pub const LAYOUT: wgpu::BindGroupLayoutDescriptor<'static> = wgpu::BindGroupLayoutDescriptor {
        label: Some("Globals Bind Group Layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    };
}
