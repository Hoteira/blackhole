use glam::Vec3;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

impl MeshVertex {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: size_of::<Self>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x3,
            1 => Float32x3,
        ],
    };
}

pub fn uv_sphere(center: Vec3, radius: f32, rings: u32, segments: u32) -> Vec<MeshVertex> {
    use std::f32::consts::{PI, TAU};

    let unit = |theta: f32, phi: f32| -> Vec3 {
        Vec3::new(
            theta.sin() * phi.cos(),
            theta.cos(),
            theta.sin() * phi.sin(),
        )
    };

    let mut vertices = Vec::with_capacity((rings * segments * 6) as usize);

    let mut push = |normal: Vec3| {
        vertices.push(MeshVertex {
            position: (center + normal * radius).to_array(),
            normal: normal.to_array(),
        });
    };

    for ring in 0..rings {
        let theta1 = PI * ring as f32 / rings as f32;
        let theta2 = PI * (ring + 1) as f32 / rings as f32;

        for seg in 0..segments {
            let phi1 = TAU * seg as f32 / segments as f32;
            let phi2 = TAU * (seg + 1) as f32 / segments as f32;

            let a = unit(theta1, phi1);
            let b = unit(theta2, phi1);
            let c = unit(theta2, phi2);
            let d = unit(theta1, phi2);

            if ring == 0 {
                push(a);
                push(c);
                push(b);
            } else if ring == rings - 1 {
                push(a);
                push(d);
                push(c);
            } else {
                push(a);
                push(c);
                push(b);

                push(a);
                push(d);
                push(c);
            }
        }
    }

    vertices
}

pub const SCHWARZSCHILD_RADIUS: f32 = 1.0;

pub fn square_grid(half_extent: f32, cells: u32, samples: u32, y: f32) -> Vec<MeshVertex> {
    let mut vertices = Vec::with_capacity((cells as usize + 1) * samples as usize * 4);

    let mut segment = |a: Vec3, b: Vec3| {
        vertices.push(MeshVertex {
            position: a.to_array(),
            normal: [0.0, 1.0, 0.0],
        });
        vertices.push(MeshVertex {
            position: b.to_array(),
            normal: [0.0, 1.0, 0.0],
        });
    };

    let coord = |i: u32, n: u32| -> f32 { -half_extent + 2.0 * half_extent * i as f32 / n as f32 };

    for line in 0..=cells {
        let fixed = coord(line, cells);

        for s in 0..samples {
            let a = coord(s, samples);
            let b = coord(s + 1, samples);

            segment(Vec3::new(fixed, y, a), Vec3::new(fixed, y, b));
            segment(Vec3::new(a, y, fixed), Vec3::new(b, y, fixed));
        }
    }

    vertices
}
