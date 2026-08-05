use glam::Vec3;

pub const C: f32 = 6.0;
pub const SPEED: f32 = 0.3 * C;
pub const MAX_PARTICLES: usize = 2_000_000;
pub const BURST: usize = 40_000;

const SPAWN_DISTANCE: f32 = 11.0;
const IMPACT_MIN: f32 = 4.5;
const IMPACT_MAX: f32 = 6.2;
const DISC_THICKNESS: f32 = 0.04;
const DESPAWN_DISTANCE: f32 = 120.0;

const ACCRETION_RADIUS: f32 = 8.0;
const RADIAL_DAMPING: f32 = 1.1;
const TANGENTIAL_DRAG: f32 = 0.0015;

const WORKGROUP_SIZE: u32 = 64;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuParticle {
    pub position: [f32; 3],
    pub alive: f32,
    pub velocity: [f32; 3],
    pub _pad: f32,
}

impl GpuParticle {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: size_of::<Self>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x3,
            1 => Float32,
            2 => Float32x3,
        ],
    };
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct SimParams {
    hole: [f32; 4],
    config: [f32; 4],
    tuning: [f32; 4],
}

impl SimParams {
    fn new(hole: Vec3, r_s: f32, dt: f32, half_extent: f32) -> Self {
        Self {
            hole: [hole.x, hole.y, hole.z, r_s],
            config: [dt, half_extent, DESPAWN_DISTANCE, ACCRETION_RADIUS],
            tuning: [RADIAL_DAMPING, TANGENTIAL_DRAG, C, 0.0],
        }
    }
}

pub struct ParticleSystem {
    pub buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::ComputePipeline,
    cursor: usize,
    high_water: usize,
    scratch: Vec<GpuParticle>,
    rng: u32,
}

impl ParticleSystem {
    pub fn new(device: &wgpu::Device, shader: &wgpu::ShaderModule) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Particle Storage Buffer"),
            size: (MAX_PARTICLES * size_of::<GpuParticle>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sim Params Buffer"),
            size: size_of::<SimParams>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Particle Compute Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Particle Compute Bind Group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: params_buffer.as_entire_binding(),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Particle Compute Pipeline Layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Particle Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: shader,
            entry_point: Some("cs_update"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            buffer,
            params_buffer,
            bind_group,
            pipeline,
            cursor: 0,
            high_water: 0,
            scratch: Vec::new(),
            rng: 0x9E3779B9,
        }
    }

    fn next_f32(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / 16_777_216.0
    }

    pub fn drawable(&self) -> u32 {
        self.high_water as u32
    }

    pub fn spawn_burst(&mut self, queue: &wgpu::Queue, hole: Vec3, beam: Vec3) {
        let forward = beam.normalize_or_zero();
        if forward == Vec3::ZERO {
            return;
        }

        let helper = if forward.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
        let right = forward.cross(helper).normalize();
        let up = right.cross(forward);
        let origin = hole - forward * SPAWN_DISTANCE;

        self.scratch.clear();

        for _ in 0..BURST {
            let impact = IMPACT_MIN + (IMPACT_MAX - IMPACT_MIN) * self.next_f32();
            let thickness = DISC_THICKNESS * (self.next_f32() * 2.0 - 1.0);

            let position = origin + right * impact + up * thickness;
            let velocity = forward * SPEED;

            self.scratch.push(GpuParticle {
                position: position.to_array(),
                alive: 1.0,
                velocity: velocity.to_array(),
                _pad: 0.0,
            });
        }

        let stride = size_of::<GpuParticle>();
        let first = (MAX_PARTICLES - self.cursor).min(BURST);

        queue.write_buffer(
            &self.buffer,
            (self.cursor * stride) as wgpu::BufferAddress,
            bytemuck::cast_slice(&self.scratch[..first]),
        );

        if first < BURST {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.scratch[first..]));
        }

        self.cursor = (self.cursor + BURST) % MAX_PARTICLES;
        self.high_water = (self.high_water + BURST).min(MAX_PARTICLES);
    }

    pub fn dispatch(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        hole: Vec3,
        r_s: f32,
        half_extent: f32,
        dt: f32,
    ) {
        if self.high_water == 0 {
            return;
        }

        queue.write_buffer(
            &self.params_buffer,
            0,
            bytemuck::bytes_of(&SimParams::new(hole, r_s, dt, half_extent)),
        );

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Particle Compute Pass"),
            timestamp_writes: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups((self.high_water as u32).div_ceil(WORKGROUP_SIZE), 1, 1);
    }
}
