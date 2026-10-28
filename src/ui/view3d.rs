use crate::{
    app::{CameraMode, RoomPlannerApp, TimeOfDay, PIXELS_PER_METER, WALL_HEIGHT, WALL_THICKNESS},
    furniture::{lamp, mesh::LocalMesh, model::ModelMesh, Part},
    geometry::rooms::triangulate,
    models::{Furniture, FurnitureKind, Opening, OpeningKind, Wall},
    ui::theme::TEXT_MUTED,
};
use eframe::{egui, egui_wgpu, wgpu};
use glam::{Mat4, Vec3};

const MAX_LIGHTS: usize = 32;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const EYE_HEIGHT: f32 = 1.6;
/// Wall height in orbit view with "cut walls" on, so you can see into the rooms.
const CUT_WALL_HEIGHT: f32 = 1.1;

// --- 1. WGSL SHADER CODE ---
const SHADER_SOURCE: &str = "
struct Light {
    position: vec4<f32>,
    color: vec4<f32>,      // rgb premultiplied by intensity, w = range
};

struct Scene {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    ambient: vec4<f32>,    // rgb, w = number of lights
    sun: vec4<f32>,        // direction, w = strength
    lights: array<Light, 32>,
};
@group(0) @binding(0) var<uniform> scene: Scene;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) emissive: f32,
    @location(4) material: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) world: vec3<f32>,
    @location(3) emissive: f32,
    @location(4) material: f32,
};

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = scene.view_proj * vec4<f32>(model.position, 1.0);
    out.color = model.color;
    out.normal = model.normal;
    out.world = model.position;
    out.emissive = model.emissive;
    out.material = model.material;
    return out;
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

// Plank flooring: staggered boards with slightly different tones, grain and dark joints.
fn wood(world: vec3<f32>, base: vec3<f32>) -> vec3<f32> {
    let plank_width = 0.19;
    let plank_length = 1.6;
    let row = floor(world.z / plank_width);
    let offset = hash(vec2<f32>(row, 3.7)) * plank_length;
    let along = world.x + offset;
    let col = floor(along / plank_length);
    let id = vec2<f32>(row, col);

    let tone = 0.86 + 0.26 * hash(id);
    let wobble = sin(along * 2.3 + hash(id) * 40.0) * 0.6;
    let grain = 0.94 + 0.06 * sin((world.z + wobble * 0.01) * 260.0 + hash(id + 1.3) * 20.0);
    let streak = 0.97 + 0.03 * sin(along * 0.9 + world.z * 30.0);

    let across = fract(world.z / plank_width);
    let lengthwise = fract(along / plank_length);
    var joint = 1.0;
    if (across < 0.025 || lengthwise < 0.004) {
        joint = 0.72;
    }
    return base * tone * grain * streak * joint;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Two-sided surfaces: light the side facing the camera.
    var n = normalize(in.normal);
    if (dot(n, scene.camera_pos.xyz - in.world) < 0.0) {
        n = -n;
    }

    var light = scene.ambient.rgb;
    // Soft sky term: up-facing surfaces get a little more ambient.
    light = light * (0.85 + 0.15 * n.y);
    light += scene.sun.w * max(dot(n, normalize(scene.sun.xyz)), 0.0) * vec3<f32>(1.0, 0.97, 0.92);

    let count = u32(scene.ambient.w);
    for (var i = 0u; i < count; i++) {
        let l = scene.lights[i];
        let to_light = l.position.xyz - in.world;
        let dist = length(to_light);
        let range = l.color.w;
        if (dist < range) {
            let dir = to_light / max(dist, 0.0001);
            // Wrap lighting so surfaces beside a lamp still catch some light.
            let ndl = max((dot(n, dir) + 0.25) / 1.25, 0.0);
            let window = pow(clamp(1.0 - pow(dist / range, 4.0), 0.0, 1.0), 2.0);
            light += l.color.rgb * ndl * window * 1.4 / (1.0 + dist * dist);
        }
    }

    var albedo = in.color;
    if (in.material > 0.5) {
        albedo = wood(in.world, in.color);
    }
    let color = albedo * light + albedo * in.emissive;
    return vec4<f32>(min(color, vec3<f32>(1.0)), 1.0);
}";

// --- 2. RUST DATA STRUCTURES ---
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
    normal: [f32; 3],
    emissive: f32,
    /// 0 = plain color, 1 = wooden floor planks.
    material: f32,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuLight {
    position: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct SceneUniform {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 4],
    ambient: [f32; 4],
    sun: [f32; 4],
    lights: [GpuLight; MAX_LIGHTS],
}

struct RenderResources {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
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
    scene: SceneUniform,
}

// --- 3. THE GPU BRIDGE ---

/// Creates the 3D pipeline once at startup. eframe must be configured with
/// `depth_buffer: 32` so the render pass has a matching depth attachment.
pub fn init(render_state: &egui_wgpu::RenderState) {
    let device = &render_state.device;

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("3D Shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER_SOURCE.into()),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        label: Some("scene_bind_group_layout"),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("3D Pipeline Layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
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
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Float32, 4 => Float32],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: render_state.target_format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
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

    render_state
        .renderer
        .write()
        .callback_resources
        .insert(RenderResources {
            pipeline,
            bind_group_layout,
        });
}

impl egui_wgpu::CallbackTrait for True3dCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(render_resources) = resources.get::<RenderResources>() else {
            return Vec::new();
        };
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
        let scene_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Scene Buffer"),
            contents: bytemuck::cast_slice(&[self.scene]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &render_resources.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: scene_buffer.as_entire_binding(),
            }],
            label: Some("scene_bind_group"),
        });

        resources.insert(FrameBuffers {
            vertex_buffer,
            index_buffer,
            bind_group,
            index_count: self.indices.len() as u32,
        });
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(render_resources) = callback_resources.get::<RenderResources>() else {
            return;
        };
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

// --- 4. GEOMETRY ---

#[derive(Default)]
struct MeshBuilder {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    /// Material of the vertices being added, see `Vertex::material`.
    material: f32,
}

impl MeshBuilder {
    fn vertex(&mut self, p: Vec3, n: Vec3, color: [f32; 3], emissive: f32) -> u32 {
        self.vertices.push(Vertex {
            position: p.to_array(),
            color,
            normal: n.to_array(),
            emissive,
            material: self.material,
        });
        self.vertices.len() as u32 - 1
    }

    /// Adds a flat quad from four corners in order.
    fn quad(&mut self, corners: [Vec3; 4], normal: Vec3, color: [f32; 3]) {
        let base = self.vertices.len() as u32;
        for c in corners {
            self.vertex(c, normal, color, 0.0);
        }
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn triangle(&mut self, corners: [Vec3; 3], normal: Vec3, color: [f32; 3]) {
        for c in corners {
            let i = self.vertex(c, normal, color, 0.0);
            self.indices.push(i);
        }
    }

    /// A box standing on the floor plan: from `origin` along `u` for `u_len` and along `v`
    /// for `v_len` (unit vectors in the x/z plane, meters), between heights `y0` and `y1`.
    #[allow(clippy::too_many_arguments)]
    fn oriented_box(
        &mut self,
        origin: egui::Vec2,
        u: egui::Vec2,
        u_len: f32,
        v: egui::Vec2,
        v_len: f32,
        (y0, y1): (f32, f32),
        color: [f32; 3],
        emissive: f32,
    ) {
        if u_len <= 1e-4 || v_len <= 1e-4 || y1 <= y0 {
            return;
        }
        let p = |c: egui::Vec2, y: f32| Vec3::new(c.x, y, c.y);
        let c = [
            origin,
            origin + u * u_len,
            origin + u * u_len + v * v_len,
            origin + v * v_len,
        ];
        let (nu, nv) = (Vec3::new(u.x, 0.0, u.y), Vec3::new(v.x, 0.0, v.y));
        let faces = [
            ([p(c[0], y0), p(c[1], y0), p(c[1], y1), p(c[0], y1)], -nv),
            ([p(c[3], y0), p(c[2], y0), p(c[2], y1), p(c[3], y1)], nv),
            ([p(c[0], y0), p(c[3], y0), p(c[3], y1), p(c[0], y1)], -nu),
            ([p(c[1], y0), p(c[2], y0), p(c[2], y1), p(c[1], y1)], nu),
            (c.map(|c| p(c, y1)), Vec3::Y),
            (c.map(|c| p(c, y0)), -Vec3::Y),
        ];
        for (corners, normal) in faces {
            let base = self.vertices.len() as u32;
            for corner in corners {
                self.vertex(corner, normal, color, emissive);
            }
            self.indices
                .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    /// A wall of real thickness with holes for its windows and doors.
    fn wall(&mut self, wall: &Wall, height: f32, color: [f32; 3]) {
        let a = egui::vec2(wall.start.x, wall.start.y) / PIXELS_PER_METER;
        let b = egui::vec2(wall.end.x, wall.end.y) / PIXELS_PER_METER;
        let length = (b - a).length();
        if length < 1e-4 {
            return;
        }
        let dir = (b - a) / length;
        let across = dir.rot90();
        let half = WALL_THICKNESS / 2.0;
        // Wall pieces between `from` and `to` meters along the wall (ends extended so corners close).
        let mut piece = |builder: &mut Self, from: f32, to: f32, y0: f32, y1: f32| {
            builder.oriented_box(
                a + dir * from - across * half,
                dir,
                to - from,
                across,
                WALL_THICKNESS,
                (y0, y1),
                color,
                0.0,
            );
        };

        let mut openings: Vec<(f32, f32, &Opening)> = wall
            .openings
            .iter()
            .map(|o| {
                let center = o.clamped_offset(length);
                (center - o.width / 2.0, center + o.width / 2.0, o)
            })
            .collect();
        openings.sort_by(|x, y| x.0.total_cmp(&y.0));

        let mut cursor = -half;
        for (from, to, opening) in openings {
            let from = from.max(cursor);
            if to <= from {
                continue;
            }
            piece(self, cursor, from, 0.0, height);
            piece(self, from, to, 0.0, opening.sill.min(height));
            piece(
                self,
                from,
                to,
                (opening.sill + opening.height).min(height),
                height,
            );
            self.opening(a, dir, from, to, opening, height);
            cursor = to;
        }
        piece(self, cursor, length + half, 0.0, height);
    }

    /// Frame and glass, or an open door leaf, filling an opening between `from` and `to`.
    fn opening(
        &mut self,
        a: egui::Vec2,
        dir: egui::Vec2,
        from: f32,
        to: f32,
        opening: &Opening,
        height: f32,
    ) {
        const FRAME: [f32; 3] = [0.97, 0.97, 0.96];
        const GLASS: [f32; 3] = [0.62, 0.77, 0.88];
        const DOOR: [f32; 3] = [0.93, 0.92, 0.9];
        let across = dir.rot90();
        let bottom = opening.sill;
        let top = (opening.sill + opening.height).min(height);
        if top <= bottom {
            return;
        }
        let frame = 0.05;
        let depth = 0.07;
        let back = a - across * depth / 2.0;

        if opening.kind == OpeningKind::Window {
            // Jambs, sill, head and a glass pane in the middle of the wall.
            self.oriented_box(
                back + dir * from,
                dir,
                frame,
                across,
                depth,
                (bottom, top),
                FRAME,
                0.0,
            );
            self.oriented_box(
                back + dir * (to - frame),
                dir,
                frame,
                across,
                depth,
                (bottom, top),
                FRAME,
                0.0,
            );
            self.oriented_box(
                back + dir * from,
                dir,
                to - from,
                across,
                depth,
                (bottom, bottom + frame),
                FRAME,
                0.0,
            );
            if top >= opening.sill + opening.height - 1e-3 {
                self.oriented_box(
                    back + dir * from,
                    dir,
                    to - from,
                    across,
                    depth,
                    (top - frame, top),
                    FRAME,
                    0.0,
                );
            }
            if to - from > 1.0 {
                let mid = (from + to) / 2.0 - frame / 2.0;
                self.oriented_box(
                    back + dir * mid,
                    dir,
                    frame,
                    across,
                    depth,
                    (bottom, top),
                    FRAME,
                    0.0,
                );
            }
            self.oriented_box(
                a - across * 0.006 + dir * from,
                dir,
                to - from,
                across,
                0.012,
                (bottom + frame, top - frame),
                GLASS,
                0.3,
            );
            return;
        }

        // Doors stand open at 90° on their swing side, so you can walk through in 3D.
        let swing = if opening.flip_side { -across } else { across };
        let width = to - from;
        let (hinge, toward_free) = if opening.flip_hinge {
            (to, -dir)
        } else {
            (from, dir)
        };
        let leaf_origin = a + dir * hinge + swing * WALL_THICKNESS / 2.0;
        let (color, glow) = if opening.kind == OpeningKind::GlassDoor {
            (GLASS, 0.3)
        } else {
            (DOOR, 0.0)
        };
        self.oriented_box(
            leaf_origin,
            toward_free,
            0.04,
            swing,
            width - 0.02,
            (0.01, top - 0.01),
            color,
            glow,
        );
        // Door frame around the opening.
        self.oriented_box(
            back + dir * from,
            dir,
            0.03,
            across,
            depth,
            (0.0, top),
            FRAME,
            0.0,
        );
        self.oriented_box(
            back + dir * (to - 0.03),
            dir,
            0.03,
            across,
            depth,
            (0.0, top),
            FRAME,
            0.0,
        );
        if top >= opening.sill + opening.height - 1e-3 {
            self.oriented_box(
                back + dir * from,
                dir,
                width,
                across,
                depth,
                (top - 0.03, top),
                FRAME,
                0.0,
            );
        }
    }

    fn polygon(&mut self, polygon: &[egui::Pos2], y: f32, normal: Vec3, color: [f32; 3]) {
        for tri in triangulate(polygon) {
            let corners = tri.map(|p| Vec3::new(p.x / PIXELS_PER_METER, y, p.y / PIXELS_PER_METER));
            self.triangle(corners, normal, color);
        }
    }

    /// Adds tessellated furniture parts, moved into place.
    fn furniture(&mut self, item: &Furniture, parts: &[Part]) {
        let mut local = LocalMesh::default();
        for part in parts {
            local.add_part(part, 28);
        }
        let base = self.vertices.len() as u32;
        for i in 0..local.positions.len() {
            self.vertex(
                local_to_3d(item, local.positions[i]),
                local_normal_to_3d(item, local.normals[i]),
                local.colors[i],
                local.emissive[i],
            );
        }
        self.indices.extend(local.indices.iter().map(|i| base + i));
    }

    fn model(&mut self, item: &Furniture, mesh: &ModelMesh) {
        let scale = Vec3::from(mesh.size).max(Vec3::splat(1e-6));
        let scale = Vec3::new(item.width, item.height, item.depth) / scale;
        let base = self.vertices.len() as u32;
        for i in 0..mesh.positions.len() {
            // Inverse scale keeps normals perpendicular under non-uniform scaling.
            let normal = local_normal_to_3d(item, Vec3::from(mesh.normals[i]) / scale);
            self.vertex(
                local_to_3d(item, Vec3::from(mesh.positions[i]) * scale),
                normal,
                mesh.colors[i],
                0.0,
            );
        }
        self.indices.extend(mesh.indices.iter().map(|i| base + i));
    }
}

fn srgb(color: [u8; 3]) -> [f32; 3] {
    color.map(|c| c as f32 / 255.0)
}

/// Local furniture space (meters) to 3D world space, where the 2D canvas maps to the x/z plane.
fn local_to_3d(item: &Furniture, p: Vec3) -> Vec3 {
    let world = item.local_to_world(p.x, p.z);
    Vec3::new(
        world.x / PIXELS_PER_METER,
        p.y + item.elevation,
        world.y / PIXELS_PER_METER,
    )
}

fn local_normal_to_3d(item: &Furniture, n: Vec3) -> Vec3 {
    let xz = egui::emath::Rot2::from_angle(item.rotation) * egui::vec2(n.x, n.z);
    Vec3::new(xz.x, n.y, xz.y).normalize_or_zero()
}

struct Lighting {
    ambient: [f32; 3],
    sun: f32,
    /// Lamps matter less in daylight, like in reality.
    lamps: f32,
    sky: egui::Color32,
}

fn lighting(time: TimeOfDay) -> Lighting {
    match time {
        TimeOfDay::Day => Lighting {
            ambient: [0.66, 0.66, 0.68],
            sun: 0.38,
            lamps: 0.3,
            sky: egui::Color32::from_rgb(214, 228, 242),
        },
        TimeOfDay::Evening => Lighting {
            ambient: [0.24, 0.22, 0.24],
            sun: 0.1,
            lamps: 0.75,
            sky: egui::Color32::from_rgb(70, 66, 90),
        },
        TimeOfDay::Night => Lighting {
            ambient: [0.09, 0.09, 0.12],
            sun: 0.0,
            lamps: 1.0,
            sky: egui::Color32::from_rgb(16, 18, 28),
        },
    }
}

// --- 5. THE PANEL ---

pub fn show_panel(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    egui::Panel::right("view3d_panel")
        .resizable(true)
        .default_size((ui.available_width() * 0.36).clamp(260.0, 720.0))
        .size_range(220.0..=1200.0)
        .show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.heading("3D");
                ui.add_space(4.0);
                let before = app.camera_mode;
                ui.selectable_value(&mut app.camera_mode, CameraMode::Orbit, "🌐 Overview")
                    .on_hover_text("Look at the whole home from above");
                ui.selectable_value(&mut app.camera_mode, CameraMode::Walk, "🚶 Walk")
                    .on_hover_text("Stand in the room. Drag the person on the plan to move.");
                if before != app.camera_mode && app.camera_mode == CameraMode::Walk {
                    if !app.visible_world_rect.contains(app.camera_pos) {
                        app.camera_pos = app.visible_world_rect.center();
                    }
                }
                ui.separator();
                ui.selectable_value(&mut app.time_of_day, TimeOfDay::Day, "☀ Day");
                ui.selectable_value(&mut app.time_of_day, TimeOfDay::Evening, "🌆 Evening");
                ui.selectable_value(&mut app.time_of_day, TimeOfDay::Night, "🌙 Night");
                if app.camera_mode == CameraMode::Orbit {
                    ui.separator();
                    ui.checkbox(&mut app.cut_walls, "Cut walls");
                    if ui.button("⛶").on_hover_text("Fit the whole plan").clicked() {
                        fit_orbit(app);
                    }
                }
            });
            let hint = match app.camera_mode {
                CameraMode::Orbit => {
                    "Drag to rotate · right-drag or Shift+drag to pan · scroll to zoom"
                }
                CameraMode::Walk => {
                    "Drag to look around · right-drag to step · scroll or arrow keys to walk"
                }
            };
            ui.label(egui::RichText::new(hint).small().color(TEXT_MUTED));
            ui.add_space(2.0);

            let (rect, response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            handle_camera_input(app, ui, &response);
            draw_scene(app, ui, rect);
        });
}

fn handle_camera_input(app: &mut RoomPlannerApp, ui: &egui::Ui, response: &egui::Response) {
    let drag = response.drag_delta();
    let scroll = if response.contains_pointer() {
        ui.ctx().input(|i| i.smooth_scroll_delta.y)
    } else {
        0.0
    };
    if response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    // Right, middle or Shift+drag pans; a plain drag rotates or looks around.
    let panning = response.dragged_by(egui::PointerButton::Secondary)
        || response.dragged_by(egui::PointerButton::Middle)
        || (response.dragged() && ui.ctx().input(|i| i.modifiers.shift));
    let zoom = ui.ctx().input(|i| i.zoom_delta());

    match app.camera_mode {
        CameraMode::Orbit => {
            if panning {
                // Move the target along the ground so the scene follows the mouse.
                let forward = -egui::vec2(app.orbit_yaw.cos(), app.orbit_yaw.sin());
                let right = egui::vec2(-forward.y, forward.x);
                let speed = app.orbit_distance * 0.0016;
                app.orbit_pan += (-right * drag.x + forward * drag.y) * speed;
            } else {
                app.orbit_yaw += drag.x * 0.008;
                app.orbit_pitch = (app.orbit_pitch + drag.y * 0.006).clamp(0.08, 1.5);
            }
            app.orbit_distance =
                (app.orbit_distance * (-scroll * 0.002).exp() / zoom).clamp(1.5, 80.0);
        }
        CameraMode::Walk => {
            let dir = egui::vec2(app.camera_angle.cos(), app.camera_angle.sin());
            if panning {
                // Step: drag up to walk forward, sideways to step aside.
                let right = egui::vec2(-dir.y, dir.x);
                app.camera_pos += (right * drag.x - dir * drag.y) * 0.012 * PIXELS_PER_METER;
            } else {
                app.camera_angle += drag.x * 0.005;
                app.camera_pitch = (app.camera_pitch - drag.y * 0.005).clamp(-1.3, 1.3);
            }
            app.camera_pos += dir * scroll * 0.008 * PIXELS_PER_METER;

            if !ui.ctx().egui_wants_keyboard_input() {
                let (dt, keys) = ui.ctx().input(|i| {
                    (
                        i.stable_dt.min(0.1),
                        [
                            i.key_down(egui::Key::ArrowUp),
                            i.key_down(egui::Key::ArrowDown),
                            i.key_down(egui::Key::ArrowLeft),
                            i.key_down(egui::Key::ArrowRight),
                        ],
                    )
                });
                let [up, down, left, right] = keys;
                let speed = 1.8 * PIXELS_PER_METER * dt;
                if up {
                    app.camera_pos += dir * speed;
                }
                if down {
                    app.camera_pos -= dir * speed;
                }
                if left {
                    app.camera_angle -= 1.8 * dt;
                }
                if right {
                    app.camera_angle += 1.8 * dt;
                }
                if keys.iter().any(|&k| k) {
                    ui.ctx().request_repaint();
                }
            }
        }
    }
}

/// Center of everything in the plan, in 3D meters.
fn plan_center(app: &RoomPlannerApp) -> Vec3 {
    let bounds = plan_bounds(app);
    let c = if bounds.is_positive() {
        bounds.center()
    } else {
        app.visible_world_rect.center()
    };
    Vec3::new(c.x / PIXELS_PER_METER, 0.8, c.y / PIXELS_PER_METER)
}

fn plan_bounds(app: &RoomPlannerApp) -> egui::Rect {
    let mut bounds = egui::Rect::NOTHING;
    for wall in &app.walls {
        bounds.extend_with(wall.start);
        bounds.extend_with(wall.end);
    }
    for item in &app.furniture {
        bounds.extend_with(item.position);
    }
    bounds
}

/// Sets the overview camera distance so the whole plan is in view.
pub fn fit_orbit(app: &mut RoomPlannerApp) {
    app.orbit_pan = egui::Vec2::ZERO;
    let bounds = plan_bounds(app);
    if bounds.is_positive() {
        let extent = bounds.size().max_elem() / PIXELS_PER_METER;
        // A tall, narrow view needs to back off further to fit the plan's width.
        let narrow = (1.0 / app.view3d_aspect.max(0.1)).max(1.0);
        app.orbit_distance = (extent * 1.1 * narrow + 2.0).clamp(1.5, 80.0);
    }
}

fn draw_scene(app: &mut RoomPlannerApp, ui: &mut egui::Ui, rect: egui::Rect) {
    let env = lighting(app.time_of_day);
    ui.painter().rect_filled(rect, 6.0, env.sky);

    let aspect = rect.width() / rect.height().max(1.0);
    app.view3d_aspect = aspect;
    let (eye, target) = match app.camera_mode {
        CameraMode::Orbit => {
            let target = plan_center(app);
            let offset = Vec3::new(
                app.orbit_pitch.cos() * app.orbit_yaw.cos(),
                app.orbit_pitch.sin(),
                app.orbit_pitch.cos() * app.orbit_yaw.sin(),
            ) * app.orbit_distance;
            let target = target + Vec3::new(app.orbit_pan.x, 0.0, app.orbit_pan.y);
            (target + offset, target)
        }
        CameraMode::Walk => {
            let eye = Vec3::new(
                app.camera_pos.x / PIXELS_PER_METER,
                EYE_HEIGHT,
                app.camera_pos.y / PIXELS_PER_METER,
            );
            let (yaw, pitch) = (app.camera_angle, app.camera_pitch);
            let dir = Vec3::new(
                pitch.cos() * yaw.cos(),
                pitch.sin(),
                pitch.cos() * yaw.sin(),
            );
            (eye, eye + dir)
        }
    };
    let view = Mat4::look_at_rh(eye, target, Vec3::Y);
    let proj = Mat4::perspective_rh(60.0_f32.to_radians(), aspect, 0.05, 300.0);

    let mut mesh = MeshBuilder::default();
    let walk = app.camera_mode == CameraMode::Walk;

    // Ground outside, floors and (when walking) ceilings inside the rooms.
    let s = 200.0;
    mesh.quad(
        [
            Vec3::new(-s, -0.01, -s),
            Vec3::new(s, -0.01, -s),
            Vec3::new(s, -0.01, s),
            Vec3::new(-s, -0.01, s),
        ],
        Vec3::Y,
        [0.78, 0.80, 0.76],
    );
    for room in &app.rooms {
        mesh.material = if app.finishes.floor.is_wood() {
            1.0
        } else {
            0.0
        };
        mesh.polygon(room, 0.0, Vec3::Y, srgb(app.finishes.floor.color()));
        mesh.material = 0.0;
        if walk {
            mesh.polygon(room, WALL_HEIGHT, -Vec3::Y, [0.96, 0.96, 0.95]);
        }
    }

    let wall_height = if !walk && app.cut_walls {
        CUT_WALL_HEIGHT
    } else {
        WALL_HEIGHT
    };
    for wall in &app.walls {
        mesh.wall(wall, wall_height, srgb(app.finishes.wall_color));
    }

    let mut lights = Vec::new();
    for item in &app.furniture {
        if let FurnitureKind::Model { path } = &item.kind {
            match app.models.get(path) {
                Ok(model) => mesh.model(item, &model),
                // Missing or broken file: show its bounding box instead.
                Err(_) => mesh.furniture(
                    item,
                    &[Part::cuboid(
                        [-item.width / 2.0, 0.0, -item.depth / 2.0],
                        [item.width / 2.0, item.height, item.depth / 2.0],
                        [230, 120, 120],
                    )],
                ),
            }
        }
        let mut parts = item.parts();
        for part in &mut parts {
            // Lift parts on the floor a hair so rugs don't flicker against it.
            part.min[1] = part.min[1].max(0.002);
        }
        mesh.furniture(item, &parts);
        if let Some(light) = lamp::light(item) {
            let pos = local_to_3d(item, Vec3::from(light.local));
            lights.push((pos, light));
        }
    }

    // The shader takes a fixed number of lights; keep the ones closest to the camera.
    lights.sort_by(|a, b| {
        a.0.distance_squared(eye)
            .total_cmp(&b.0.distance_squared(eye))
    });
    let mut scene = SceneUniform {
        view_proj: (proj * view).to_cols_array_2d(),
        camera_pos: [eye.x, eye.y, eye.z, 1.0],
        ambient: [
            env.ambient[0],
            env.ambient[1],
            env.ambient[2],
            lights.len().min(MAX_LIGHTS) as f32,
        ],
        sun: [0.35, 0.8, 0.45, env.sun],
        lights: [GpuLight {
            position: [0.0; 4],
            color: [0.0; 4],
        }; MAX_LIGHTS],
    };
    for (slot, (pos, light)) in scene.lights.iter_mut().zip(&lights) {
        slot.position = [pos.x, pos.y, pos.z, 1.0];
        let c = light.color.map(|c| c * env.lamps);
        slot.color = [c[0], c[1], c[2], light.range];
    }

    let callback = egui_wgpu::Callback::new_paint_callback(
        rect,
        True3dCallback {
            vertices: mesh.vertices,
            indices: mesh.indices,
            scene,
        },
    );
    ui.painter().add(callback);
}
