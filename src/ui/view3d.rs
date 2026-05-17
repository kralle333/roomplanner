use crate::app::{RoomPlannerApp, PIXELS_PER_METER};
use eframe::{egui, egui_wgpu, wgpu};
use glam::{Mat4, Vec3};

// --- 1. WGSL SHADER CODE ---
const SHADER_SOURCE: &str = "
struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(model.position, 1.0);
    out.color = model.color;
    out.normal = model.normal;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Brighter lighting so white walls don't look grey
    let light_dir = normalize(vec3<f32>(0.3, 0.8, 0.4));
    let ambient = 0.75; // Much brighter base shadow
    let diffuse = max(dot(in.normal, light_dir), 0.0);
    let lighting = ambient + (diffuse * 0.35); // Softer directional light
    
    return vec4<f32>(in.color * lighting, 1.0);
}";

// --- 2. RUST DATA STRUCTURES ---
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
    normal: [f32; 3],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

struct RenderResources {
    pipeline: wgpu::RenderPipeline,
    camera_bind_group_layout: wgpu::BindGroupLayout,
}

struct FrameBuffers {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    index_count: u32,
}

struct True3dCallback {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    camera_uniform: CameraUniform,
}

// --- 3. THE GPU BRIDGE ---
impl egui_wgpu::CallbackTrait for True3dCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if resources.get::<RenderResources>().is_none() {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("3D Shader"),
                source: wgpu::ShaderSource::Wgsl(SHADER_SOURCE.into()),
            });

            let camera_bind_group_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                    label: Some("camera_bind_group_layout"),
                });

            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("3D Pipeline Layout"),
                bind_group_layouts: &[Some(&camera_bind_group_layout)],
                immediate_size: 0, // FIX: Added the missing immediate_size field
            });

            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("3D Render Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3],
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Bgra8Unorm, // FIX: Fallback to the standard eframe format
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None, // FIX: Passed None instead of integer 0
                cache: None,
            });

            resources.insert(RenderResources {
                pipeline,
                camera_bind_group_layout,
            });
        }

        if self.vertices.is_empty() || self.indices.is_empty() {
            return Vec::new();
        }

        use wgpu::util::DeviceExt;
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&self.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(&self.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[self.camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let render_resources = resources.get::<RenderResources>().unwrap();

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &render_resources.camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("camera_bind_group"),
        });

        resources.insert(FrameBuffers {
            vertex_buffer,
            index_buffer,
            bind_group,
            index_count: self.indices.len() as u32,
        });

        Vec::new()
    }

    // FIX: Removed the <'a> lifetime to strictly match the trait signature's 'static render_pass
    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let render_resources = callback_resources.get::<RenderResources>().unwrap();

        if let Some(frame_buffers) = callback_resources.get::<FrameBuffers>() {
            if frame_buffers.index_count > 0 {
                render_pass.set_pipeline(&render_resources.pipeline);
                render_pass.set_bind_group(0, &frame_buffers.bind_group, &[]);
                render_pass.set_vertex_buffer(0, frame_buffers.vertex_buffer.slice(..));
                render_pass.set_index_buffer(
                    frame_buffers.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint32,
                );
                render_pass.draw_indexed(0..frame_buffers.index_count, 0, 0..1);
            }
        }
    }
}

// --- 4. THE UI & GEOMETRY GENERATOR ---
pub fn process_and_draw(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    let speed = 4.0 / app.zoom_factor;
    let rot_speed = 0.05;

    ui.ctx().input(|i| {
        if i.key_down(egui::Key::ArrowLeft) {
            app.camera_angle -= rot_speed;
        }
        if i.key_down(egui::Key::ArrowRight) {
            app.camera_angle += rot_speed;
        }

        let dir = egui::vec2(app.camera_angle.cos(), app.camera_angle.sin());
        if i.key_down(egui::Key::ArrowUp) {
            app.camera_pos += dir * speed;
        }
        if i.key_down(egui::Key::ArrowDown) {
            app.camera_pos -= dir * speed;
        }
    });

    egui::Window::new("👁 True 3D View")
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-20.0, 40.0))
        .resizable(true)
        .default_size(egui::vec2(500.0, 400.0))
        .show(ui.ctx(), |ui| {
            let (rect, _response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::drag());

            // Math: Generate Camera Matrix
            let aspect = rect.width() / rect.height();
            let cam_x = app.camera_pos.x / PIXELS_PER_METER;
            let cam_z = app.camera_pos.y / PIXELS_PER_METER;

            let eye = Vec3::new(cam_x, 1.6, cam_z); // Camera is 1.6m tall
            let pitch = -5.0_f32.to_radians(); // Tilt 5 degrees downward
            let yaw = app.camera_angle;

            // Convert Yaw and Pitch into a 3D direction vector
            let dir = Vec3::new(
                pitch.cos() * yaw.cos(),
                pitch.sin(),
                pitch.cos() * yaw.sin(),
            );

            let view = Mat4::look_at_rh(eye, eye + dir, Vec3::Y);
            let proj = Mat4::perspective_rh(60.0_f32.to_radians(), aspect, 0.1, 100.0);

            let camera_uniform = CameraUniform {
                view_proj: (proj * view).to_cols_array_2d(),
            };

            // Extrude 2D walls into 3D Mesh
            let mut vertices = Vec::new();
            let mut indices = Vec::new();

            let mut sorted_walls: Vec<_> = app.walls.iter().collect();
            sorted_walls.sort_by(|a, b| {
                let center_a = egui::pos2((a.start.x + a.end.x) / 2.0, (a.start.y + a.end.y) / 2.0);
                let center_b = egui::pos2((b.start.x + b.end.x) / 2.0, (b.start.y + b.end.y) / 2.0);
                let dist_a = app.camera_pos.distance_sq(center_a);
                let dist_b = app.camera_pos.distance_sq(center_b);
                dist_b.partial_cmp(&dist_a).unwrap()
            });

            // Wood Floor
            let floor_color = [0.54, 0.27, 0.07];
            let s = 100.0;
            let f_idx = vertices.len() as u32;
            vertices.push(Vertex {
                position: [-s, 0.0, -s],
                color: floor_color,
                normal: [0.0, 1.0, 0.0],
            });
            vertices.push(Vertex {
                position: [s, 0.0, -s],
                color: floor_color,
                normal: [0.0, 1.0, 0.0],
            });
            vertices.push(Vertex {
                position: [s, 0.0, s],
                color: floor_color,
                normal: [0.0, 1.0, 0.0],
            });
            vertices.push(Vertex {
                position: [-s, 0.0, s],
                color: floor_color,
                normal: [0.0, 1.0, 0.0],
            });
            indices.extend_from_slice(&[f_idx, f_idx + 2, f_idx + 1, f_idx, f_idx + 3, f_idx + 2]);

            // White Walls
            let wall_color = [1.0, 1.0, 1.0];
            let height = 2.5;

            for wall in sorted_walls {
                let x1 = wall.start.x / PIXELS_PER_METER;
                let z1 = wall.start.y / PIXELS_PER_METER;
                let x2 = wall.end.x / PIXELS_PER_METER;
                let z2 = wall.end.y / PIXELS_PER_METER;

                let dx = x2 - x1;
                let dz = z2 - z1;
                let len = (dx * dx + dz * dz).sqrt();
                let normal = [-dz / len, 0.0, dx / len];

                let start_idx = vertices.len() as u32;
                vertices.push(Vertex {
                    position: [x1, 0.0, z1],
                    color: wall_color,
                    normal,
                });
                vertices.push(Vertex {
                    position: [x2, 0.0, z2],
                    color: wall_color,
                    normal,
                });
                vertices.push(Vertex {
                    position: [x2, height, z2],
                    color: wall_color,
                    normal,
                });
                vertices.push(Vertex {
                    position: [x1, height, z1],
                    color: wall_color,
                    normal,
                });

                indices.extend_from_slice(&[
                    start_idx,
                    start_idx + 1,
                    start_idx + 2,
                    start_idx,
                    start_idx + 2,
                    start_idx + 3,
                ]);
            }

            // Inject into WGPU
            let callback = egui_wgpu::Callback::new_paint_callback(
                rect,
                True3dCallback {
                    vertices,
                    indices,
                    camera_uniform,
                },
            );
            ui.painter().add(callback);
        });
}
