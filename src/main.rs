use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use crate::camera::Camera;
use crate::globals::Globals;
use crate::mesh::{MeshVertex, SCHWARZSCHILD_RADIUS, square_grid, uv_sphere};
use crate::particles::{GpuParticle, ParticleSystem};

mod camera;
mod globals;
mod mesh;
mod particles;


const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const LIGHT_DIR: glam::Vec3 = glam::Vec3::new(-0.4, -0.7, -0.6);
const GRID_HALF_EXTENT: f32 = 48.0;
const GRID_Y: f32 = -4.0;
const SPIN_STEPS: [f32; 4] = [0.0, 0.5, 0.9, 0.998];

struct State {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    num_vertices: u32,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    grid_pipeline: wgpu::RenderPipeline,
    grid_buffer: wgpu::Buffer,
    num_grid_vertices: u32,
    depth_view: wgpu::TextureView,
    particle_pipeline: wgpu::RenderPipeline,
    particle_secondary_pipeline: wgpu::RenderPipeline,
    hud_pipeline: wgpu::RenderPipeline,
    tonemap_pipeline: wgpu::RenderPipeline,
    raymarch_pipeline: wgpu::RenderPipeline,
    raymarch: bool,
    spin: usize,
    bright_pipeline: wgpu::RenderPipeline,
    blur_h_pipeline: wgpu::RenderPipeline,
    blur_v_pipeline: wgpu::RenderPipeline,
    bloom_a_view: wgpu::TextureView,
    bloom_a_bind_group: wgpu::BindGroup,
    bloom_b_view: wgpu::TextureView,
    bloom_b_bind_group: wgpu::BindGroup,
    hdr_layout: wgpu::BindGroupLayout,
    hdr_view: wgpu::TextureView,
    hdr_bind_group: wgpu::BindGroup,


    particles: ParticleSystem,

    last_frame: std::time::Instant,
    camera: Camera,
    hole: glam::Vec3,
    lensing: bool,
    vsync: bool,
    frame_ms: f32,

    dragging: bool,
    last_cursor: Option<(f64, f64)>,
}

impl State {
    async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();

        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .unwrap();
        config.usage = wgpu::TextureUsages::RENDER_ATTACHMENT;

        config.present_mode = wgpu::PresentMode::AutoNoVsync;
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let hole = glam::Vec3::ZERO;

        let camera = Camera::new(hole, 18.0, config.width as f32 / config.height as f32);

        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Globals Buffer"),
            contents: bytemuck::bytes_of(&Globals::new(
                camera.view_projection(),
                LIGHT_DIR,
                camera.eye(),
                hole,
                SCHWARZSCHILD_RADIUS,
                0.0,
                config.width as f32 / config.height as f32,
                EXPOSURE,
                config.format.is_srgb(),
                camera_basis(&camera),
                (camera.fov_y * 0.5).tan(),
                1.0,
                0.0,
            )),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let globals_bind_group_layout = device.create_bind_group_layout(&Globals::LAYOUT);

        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Globals Bind Group"),
            layout: &globals_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[Some(&globals_bind_group_layout)],
            immediate_size: 0,
        });

        let vertices = uv_sphere(hole, SCHWARZSCHILD_RADIUS, 32, 48);
        let num_vertices = vertices.len() as u32;

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Sphere Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(MeshVertex::LAYOUT)],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {

                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },

            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let grid_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Grid Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_lensed"),
                buffers: &[Some(MeshVertex::LAYOUT)],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_grid"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let grid_vertices = square_grid(GRID_HALF_EXTENT, 16, 192, GRID_Y);
        let num_grid_vertices = grid_vertices.len() as u32;

        let grid_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Grid Vertex Buffer"),
            contents: bytemuck::cast_slice(&grid_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let particle_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Particle Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_particle"),
                buffers: &[Some(GpuParticle::LAYOUT)],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_particle_shaded"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::PointList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let particle_secondary_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Particle Secondary Image Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_particle_secondary"),
                    buffers: &[Some(GpuParticle::LAYOUT)],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_particle_shaded"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: HDR_FORMAT,
                        blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::PointList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });

        let hud_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("HUD Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_hud"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_hud"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            // Always passes and writes nothing: the HUD sits on top of the scene
            // regardless of what is behind it.
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let compute_shader =
            device.create_shader_module(wgpu::include_wgsl!("compute.wgsl"));
        let particles = ParticleSystem::new(&device, &compute_shader);

        let raymarch_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Raymarch Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_fullscreen"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_raymarch"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let hdr_layout = hdr_bind_group_layout(&device);
        let (hdr_view, hdr_bind_group) = create_hdr_target(&device, &config, &hdr_layout);

        let post_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Post Pipeline Layout"),
            bind_group_layouts: &[Some(&globals_bind_group_layout), Some(&hdr_layout)],
            immediate_size: 0,
        });

        let tonemap_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Tonemap Pipeline Layout"),
            bind_group_layouts: &[
                Some(&globals_bind_group_layout),
                Some(&hdr_layout),
                Some(&hdr_layout),
            ],
            immediate_size: 0,
        });

        let bright_pipeline = post_pipeline(&device, &shader, &post_layout, "fs_bright", HDR_FORMAT, "Bright Pass");
        let blur_h_pipeline = post_pipeline(&device, &shader, &post_layout, "fs_blur_h", HDR_FORMAT, "Blur H");
        let blur_v_pipeline = post_pipeline(&device, &shader, &post_layout, "fs_blur_v", HDR_FORMAT, "Blur V");

        let (bloom_a_view, bloom_a_bind_group) =
            create_sample_target(&device, config.width / 2, config.height / 2, &hdr_layout, "Bloom A");
        let (bloom_b_view, bloom_b_bind_group) =
            create_sample_target(&device, config.width / 2, config.height / 2, &hdr_layout, "Bloom B");


        let tonemap_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Tonemap Pipeline"),
            layout: Some(&tonemap_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_fullscreen"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_tonemap"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let depth_view = create_depth_view(&device, &config);

        Self {
            window,
            surface,
            device,
            queue,
            config,
            render_pipeline,
            vertex_buffer,
            num_vertices,
            globals_buffer,
            globals_bind_group,
            grid_pipeline,
            grid_buffer,
            num_grid_vertices,
            particle_pipeline,
            particle_secondary_pipeline,
            hud_pipeline,
            tonemap_pipeline,
            raymarch_pipeline,
            raymarch: true,
            spin: 0,
            bright_pipeline,
            blur_h_pipeline,
            blur_v_pipeline,
            bloom_a_view,
            bloom_a_bind_group,
            bloom_b_view,
            bloom_b_bind_group,
            hdr_layout,
            hdr_view,
            hdr_bind_group,


            particles,

            last_frame: std::time::Instant::now(),
            depth_view,
            camera,
            hole,
            lensing: true,
            vsync: false,
            frame_ms: 0.0,
            dragging: false,
            last_cursor: None,
        }
    }

    fn upload_globals(&mut self) {
        self.queue.write_buffer(
            &self.globals_buffer,
            0,
            bytemuck::bytes_of(&Globals::new(
                self.camera.view_projection(),
                LIGHT_DIR,
                self.camera.eye(),
                self.hole,
                if self.lensing { SCHWARZSCHILD_RADIUS } else { 0.0 },
                self.frame_ms,
                self.config.width as f32 / self.config.height as f32,
                EXPOSURE,
                self.config.format.is_srgb(),
                camera_basis(&self.camera),
                (self.camera.fov_y * 0.5).tan(),
                if self.raymarch { 1.0 } else { 0.0 },
                SPIN_STEPS[self.spin],
            )),
        );
    }

    fn fire(&mut self) {
        let beam = glam::Vec3::new(1.0, 0.0, 0.0);
        self.particles.spawn_burst(&self.queue, self.hole, beam);
        self.window.request_redraw();
    }

    fn update(&mut self) -> f32 {
        let now = std::time::Instant::now();
        let elapsed = (now - self.last_frame).as_secs_f32();
        let dt = elapsed.min(1.0 / 30.0);
        self.last_frame = now;

        self.frame_ms += (elapsed * 1000.0 - self.frame_ms) * 0.1;

        dt
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);

            self.depth_view = create_depth_view(&self.device, &self.config);

            let (hdr_view, hdr_bind_group) =
                create_hdr_target(&self.device, &self.config, &self.hdr_layout);
            self.hdr_view = hdr_view;
            self.hdr_bind_group = hdr_bind_group;

            let (a_view, a_bg) = create_sample_target(
                &self.device,
                self.config.width / 2,
                self.config.height / 2,
                &self.hdr_layout,
                "Bloom A",
            );
            let (b_view, b_bg) = create_sample_target(
                &self.device,
                self.config.width / 2,
                self.config.height / 2,
                &self.hdr_layout,
                "Bloom B",
            );
            self.bloom_a_view = a_view;
            self.bloom_a_bind_group = a_bg;
            self.bloom_b_view = b_view;
            self.bloom_b_bind_group = b_bg;

            self.camera.aspect = new_size.width as f32 / new_size.height as f32;
            self.upload_globals();
        }
    }

    fn orbit(&mut self, dx: f32, dy: f32) {
        const SENSITIVITY: f32 = 0.005;

        self.camera.yaw -= dx * SENSITIVITY;
        self.camera.pitch = (self.camera.pitch + dy * SENSITIVITY).clamp(-1.5, 1.5);
        self.upload_globals();
        self.window.request_redraw();
    }

    fn toggle_raymarch(&mut self) {
        self.raymarch = !self.raymarch;
        self.window.request_redraw();
    }

    fn cycle_spin(&mut self) {
        self.spin = (self.spin + 1) % SPIN_STEPS.len();
        self.window.request_redraw();
    }

    fn toggle_vsync(&mut self) {
        self.vsync = !self.vsync;

        self.config.present_mode = if self.vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        };

        self.surface.configure(&self.device, &self.config);
    }

    fn zoom(&mut self, amount: f32) {
        self.camera.distance = (self.camera.distance * (1.0 - amount * 0.1)).clamp(1.5, 100.0);
        self.upload_globals();
        self.window.request_redraw();
    }

    fn render(&mut self, dt: f32) {
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                self.resize(self.window.inner_size());
                texture
            }
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.resize(self.window.inner_size());
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return;
            }
            _ => return,
        };

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        self.particles.dispatch(
            &self.queue,
            &mut encoder,
            self.hole,
            SCHWARZSCHILD_RADIUS,
            SPIN_STEPS[self.spin],
            GRID_HALF_EXTENT,
            dt,
        );

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.hdr_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,

                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            render_pass.set_bind_group(0, &self.globals_bind_group, &[]);

            render_pass.set_pipeline(&self.raymarch_pipeline);
            render_pass.draw(0..3, 0..1);

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.globals_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.draw(0..self.num_vertices, 0..1);

            render_pass.set_pipeline(&self.grid_pipeline);
            render_pass.set_vertex_buffer(0, self.grid_buffer.slice(..));
            render_pass.draw(0..self.num_grid_vertices, 0..1);

            let drawable = self.particles.drawable();

            if drawable > 0 {
                render_pass.set_vertex_buffer(0, self.particles.buffer.slice(..));

                render_pass.set_pipeline(&self.particle_secondary_pipeline);
                render_pass.draw(0..drawable, 0..1);

                render_pass.set_pipeline(&self.particle_pipeline);
                render_pass.draw(0..drawable, 0..1);
            }

        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Bloom Bright Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.bloom_a_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(&self.bright_pipeline);
            pass.set_bind_group(0, &self.globals_bind_group, &[]);
            pass.set_bind_group(1, &self.hdr_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        for (target, source, pipeline) in [
            (&self.bloom_b_view, &self.bloom_a_bind_group, &self.blur_h_pipeline),
            (&self.bloom_a_view, &self.bloom_b_bind_group, &self.blur_v_pipeline),
        ] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Bloom Blur Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.globals_bind_group, &[]);
            pass.set_bind_group(1, source, &[]);
            pass.draw(0..3, 0..1);
        }

        {
            let mut post_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Tonemap Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            post_pass.set_pipeline(&self.tonemap_pipeline);
            post_pass.set_bind_group(0, &self.globals_bind_group, &[]);
            post_pass.set_bind_group(1, &self.hdr_bind_group, &[]);
            post_pass.set_bind_group(2, &self.bloom_a_bind_group, &[]);
            post_pass.draw(0..3, 0..1);

            post_pass.set_pipeline(&self.hud_pipeline);
            post_pass.set_bind_group(0, &self.globals_bind_group, &[]);
            post_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        self.queue.present(surface_texture);
    }
}

fn create_depth_view(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Depth Texture"),
        size: wgpu::Extent3d {
            width: config.width.max(1),
            height: config.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });

    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

struct App {
    runtime: tokio::runtime::Runtime,
    state: Option<State>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() {
            let window_attributes = Window::default_attributes()
                .with_title("godly — drag orbit, scroll zoom, space lensing, F fire, V vsync")
                .with_inner_size(winit::dpi::PhysicalSize::new(800, 600));

            let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
            let state = self.runtime.block_on(State::new(window.clone()));
            window.request_redraw();
            self.state = Some(state);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let state = match self.state.as_mut() {
            Some(state) => state,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(physical_size) => {
                state.resize(physical_size);
                state.window.request_redraw();
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: button_state,
                ..
            } => {
                state.dragging = button_state == ElementState::Pressed;
                if !state.dragging {
                    state.last_cursor = None;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let current = (position.x, position.y);

                if state.dragging {
                    if let Some(previous) = state.last_cursor {
                        state.orbit(
                            (current.0 - previous.0) as f32,
                            (current.1 - previous.1) as f32,
                        );
                    }
                }

                state.last_cursor = Some(current);
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::Space),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                state.lensing = !state.lensing;
                state.upload_globals();
                state.window.request_redraw();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::KeyF),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                state.fire();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::KeyR),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                state.toggle_raymarch();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::KeyK),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                state.cycle_spin();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::KeyV),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                state.toggle_vsync();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let amount = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 60.0,
                };
                state.zoom(amount);
            }
            WindowEvent::RedrawRequested => {
                let dt = state.update();
                state.upload_globals();
                state.render(dt);

                state.window.request_redraw();
            }
            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();

    event_loop.set_control_flow(ControlFlow::Poll);

    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut app = App {
        runtime,
        state: None,
    };

    event_loop.run_app(&mut app).unwrap();
}

const EXPOSURE: f32 = 1.0;

fn create_hdr_target(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
    layout: &wgpu::BindGroupLayout,
) -> (wgpu::TextureView, wgpu::BindGroup) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("HDR Target"),
        size: wgpu::Extent3d {
            width: config.width.max(1),
            height: config.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: HDR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("HDR Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("HDR Bind Group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });

    (view, bind_group)
}

fn hdr_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("HDR Bind Group Layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn camera_basis(camera: &Camera) -> (glam::Vec3, glam::Vec3, glam::Vec3) {
    let forward = (camera.target - camera.eye()).normalize_or_zero();
    let right = forward.cross(glam::Vec3::Y).normalize_or_zero();
    let up = right.cross(forward);

    (right, up, forward)
}

fn create_sample_target(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    layout: &wgpu::BindGroupLayout,
    label: &str,
) -> (wgpu::TextureView, wgpu::BindGroup) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: HDR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Bloom Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });

    (view, bind_group)
}

fn post_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    entry: &str,
    format: wgpu::TextureFormat,
    label: &str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_fullscreen"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(entry),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}
