use crate::{
    app::{
        CameraMode, Projection, RoomPlannerApp, TimeOfDay, PIXELS_PER_METER, WALL_HEIGHT,
        WALL_THICKNESS,
    },
    furniture::{lamp, mesh::LocalMesh, model::ModelMesh, Part},
    geometry::rooms::triangulate,
    models::{Furniture, FurnitureKind, Opening, OpeningKind, Wall},
};
use eframe::{egui, egui_wgpu, wgpu};
use glam::{Mat4, Vec3};

const MAX_LIGHTS: usize = 64;
/// Lights nearest the camera that cast shadows, each with its own depth map.
const MAX_SHADOWS: usize = 8;
const SHADOW_SIZE: u32 = 1024;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const EYE_HEIGHT: f32 = 1.6;
/// Wall height in orbit view with "cut walls" on, so you can see into the rooms.
const CUT_WALL_HEIGHT: f32 = 1.1;

// --- 1. WGSL SHADER CODE ---
const SHADER_SOURCE: &str = "
struct Light {
    position: vec4<f32>,
    color: vec4<f32>,      // rgb premultiplied by intensity, w = range
    direction: vec4<f32>,  // beam axis; w = cos of the beam's half angle, below -1 for all round
    shadow: vec4<f32>,     // x = layer of its shadow map, below 0 for none
};

struct Scene {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>, // w = 1 to draw the floor grid
    ambient: vec4<f32>,    // rgb, w = number of lights
    sun: vec4<f32>,        // direction, w = strength
    shadow_mats: array<mat4x4<f32>, 8>,
    lights: array<Light, 64>,
};
@group(0) @binding(0) var<uniform> scene: Scene;
@group(0) @binding(1) var shadow_maps: texture_depth_2d_array;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;

// How much of a light reaches a point, from that light's shadow map: 1 lit, 0 in shadow.
// Points outside the map (e.g. above a lamp that looks down) count as lit.
fn shadow_at(layer: i32, p: vec3<f32>) -> f32 {
    let c = scene.shadow_mats[layer] * vec4<f32>(p, 1.0);
    if (c.w <= 0.0) {
        return 1.0;
    }
    let ndc = c.xyz / c.w;
    if (abs(ndc.x) > 1.0 || abs(ndc.y) > 1.0 || ndc.z < 0.0 || ndc.z > 1.0) {
        return 1.0;
    }
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    let texel = 1.0 / 1024.0;
    var sum = 0.0;
    // Soft edges: average a few neighbouring samples.
    for (var x = -1; x <= 1; x++) {
        for (var y = -1; y <= 1; y++) {
            let o = vec2<f32>(f32(x), f32(y)) * texel * 1.5;
            sum += textureSampleCompareLevel(shadow_maps, shadow_sampler, uv + o, layer, ndc.z);
        }
    }
    return sum / 9.0;
}

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

// Solid wood worktop: long glued strips with fine grain running along them, lengthwise at
// `angle` (radians) on the floor plane, and faint joints between the strips.
fn worktop(world: vec3<f32>, base: vec3<f32>, angle: f32) -> vec3<f32> {
    let dir = vec2<f32>(cos(angle), sin(angle));
    let along = dot(world.xz, dir);
    let across = dot(world.xz, vec2<f32>(-dir.y, dir.x)) + world.y * 0.37;
    let strip_width = 0.042;
    let strip = floor(across / strip_width);
    let strip_length = 0.9 + 0.6 * hash(vec2<f32>(strip, 5.1));
    let shifted = along + hash(vec2<f32>(strip, 9.3)) * strip_length;
    let piece = floor(shifted / strip_length);
    let id = vec2<f32>(strip, piece);

    let tone = 0.88 + 0.22 * hash(id);
    let wave = sin(shifted * 1.7 + hash(id) * 30.0) * 0.004;
    let grain = 0.95 + 0.05 * sin((across + wave) * 520.0 + hash(id + 2.1) * 15.0);
    let fleck = 0.98 + 0.02 * sin(shifted * 23.0 + across * 140.0);
    var joint = 1.0;
    if (fract(across / strip_width) < 0.03 || fract(shifted / strip_length) < 0.003) {
        joint = 0.9;
    }
    return base * tone * grain * fleck * joint;
}

// 1 on a grid line of the given spacing (meters), fading out where lines would crowd.
fn grid_line(coord: vec2<f32>, fw: vec2<f32>, spacing: f32) -> f32 {
    let c = coord / spacing;
    let w = max(fw / spacing, vec2<f32>(1e-5, 1e-5));
    let d = abs(fract(c - 0.5) - 0.5) / w;
    let line = 1.0 - min(min(d.x, d.y), 1.0);
    let density = max(w.x, w.y);
    return line * clamp(1.0 - (density - 0.12) / 0.2, 0.0, 1.0);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Derivatives must be taken in uniform control flow, before any branching.
    let fw = fwidth(in.world.xz);
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
            // Spots light a soft-edged cone and spill a little outside it.
            var beam = 1.0;
            if (l.direction.w > -1.5) {
                let along = dot(-dir, l.direction.xyz);
                beam = 0.04 + 0.96 * smoothstep(l.direction.w, min(l.direction.w + 0.12, 1.0), along);
            }
            var lit = 1.0;
            if (l.shadow.x > -0.5) {
                // Pushed off the surface towards the light, against self-shadowing.
                var towards = n;
                if (dot(towards, dir) < 0.0) {
                    towards = -towards;
                }
                lit = shadow_at(i32(l.shadow.x), in.world + towards * 0.02 + dir * 0.01);
            }
            light += l.color.rgb * ndl * window * beam * lit * 1.4 / (1.0 + dist * dist);
        }
    }

    var albedo = in.color;
    if (in.material > 1.5) {
        albedo = worktop(in.world, in.color, (in.material - 2.0) / 0.9 * 3.14159265);
    } else if (in.material > 0.5) {
        albedo = wood(in.world, in.color);
    }
    // Grid on the floor and ground: 10 cm squares, a stronger line every meter.
    if (scene.camera_pos.w > 0.5 && abs(in.world.y) < 0.02) {
        let minor = grid_line(in.world.xz, fw, 0.1);
        let major = grid_line(in.world.xz, fw, 1.0);
        albedo = albedo * (1.0 - 0.07 * minor) * (1.0 - 0.16 * major);
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
    /// 0 = plain color, 1 = wooden floor planks, 2 to 2.9 = solid wood worktop with the
    /// grain at (material - 2) / 0.9 × 180° on the floor plane.
    material: f32,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuLight {
    position: [f32; 4],
    color: [f32; 4],
    /// Beam axis, w = cos of its half angle; w = -2 shines all round.
    direction: [f32; 4],
    /// x = layer of its shadow map, -1 for none.
    shadow: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct SceneUniform {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 4],
    ambient: [f32; 4],
    sun: [f32; 4],
    shadow_mats: [[[f32; 4]; 4]; MAX_SHADOWS],
    lights: [GpuLight; MAX_LIGHTS],
}

struct RenderResources {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    /// Renders depth from a light into one layer of `shadow_layers`.
    shadow_pipeline: wgpu::RenderPipeline,
    shadow_bind_group_layout: wgpu::BindGroupLayout,
    shadow_layers: Vec<wgpu::TextureView>,
    shadow_array: wgpu::TextureView,
    shadow_sampler: wgpu::Sampler,
}

/// Draws only depth, from a light's point of view.
const SHADOW_SHADER: &str = "
struct ShadowView {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> shadow_view: ShadowView;

@vertex
fn vs_shadow(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return shadow_view.view_proj * vec4<f32>(position, 1.0);
}
";

struct FrameBuffers {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    index_count: u32,
}

struct True3dCallback {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    /// The triangles that cast shadows: everything but the lamps themselves, which would
    /// otherwise block their own light.
    caster_indices: Vec<u32>,
    /// How many of `scene.shadow_mats` are in use.
    shadow_count: usize,
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
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
        ],
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

    // Shadow maps: one depth layer per shadow-casting light.
    let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shadow_maps"),
        size: wgpu::Extent3d {
            width: SHADOW_SIZE,
            height: SHADOW_SIZE,
            depth_or_array_layers: MAX_SHADOWS as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let shadow_layers = (0..MAX_SHADOWS as u32)
        .map(|layer| {
            shadow_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("shadow_layer"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            })
        })
        .collect();
    let shadow_array = shadow_texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("shadow_array"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("shadow_sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        compare: Some(wgpu::CompareFunction::LessEqual),
        ..Default::default()
    });
    let shadow_bind_group_layout =
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
            label: Some("shadow_bind_group_layout"),
        });
    let shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Shadow Shader"),
        source: wgpu::ShaderSource::Wgsl(SHADOW_SHADER.into()),
    });
    let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Shadow Pipeline Layout"),
        bind_group_layouts: &[Some(&shadow_bind_group_layout)],
        immediate_size: 0,
    });
    let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Shadow Pipeline"),
        layout: Some(&shadow_layout),
        vertex: wgpu::VertexState {
            module: &shadow_shader,
            entry_point: Some("vs_shadow"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3],
            }],
        },
        fragment: None,
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
            // Against surfaces shadowing themselves.
            bias: wgpu::DepthBiasState {
                constant: 2,
                slope_scale: 2.0,
                clamp: 0.0,
            },
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
            shadow_pipeline,
            shadow_bind_group_layout,
            shadow_layers,
            shadow_array,
            shadow_sampler,
        });
}

impl egui_wgpu::CallbackTrait for True3dCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        egui_encoder: &mut wgpu::CommandEncoder,
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
        // Depth from each shadow-casting light, before the scene is drawn.
        if !self.caster_indices.is_empty() {
            let caster_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Shadow Caster Index Buffer"),
                contents: bytemuck::cast_slice(&self.caster_indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            for layer in 0..self.shadow_count.min(MAX_SHADOWS) {
                let view_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Shadow View Buffer"),
                    contents: bytemuck::cast_slice(&self.scene.shadow_mats[layer]),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let view_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: &render_resources.shadow_bind_group_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: view_buffer.as_entire_binding(),
                    }],
                    label: Some("shadow_view_bind_group"),
                });
                let mut pass = egui_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Shadow Pass"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &render_resources.shadow_layers[layer],
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
                pass.set_pipeline(&render_resources.shadow_pipeline);
                pass.set_bind_group(0, &view_group, &[]);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_index_buffer(caster_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.caster_indices.len() as u32, 0, 0..1);
            }
        }

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &render_resources.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: scene_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&render_resources.shadow_array),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&render_resources.shadow_sampler),
                },
            ],
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
    /// Color for the top faces of boxes, e.g. cut walls drawn dark like on a plan.
    top_color: Option<[f32; 3]>,
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
            let color = if normal == Vec3::Y {
                self.top_color.unwrap_or(color)
            } else {
                color
            };
            for corner in corners {
                self.vertex(corner, normal, color, emissive);
            }
            self.indices
                .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    /// A wall of real thickness with holes for its windows and doors.
    /// `ends`: how far to draw past the start and end, see `wall_end_extensions`.
    fn wall(
        &mut self,
        wall: &Wall,
        ends: (f32, f32),
        height: f32,
        color: [f32; 3],
        top: Option<[f32; 3]>,
    ) {
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
            builder.top_color = if y1 >= height { top } else { None };
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
            builder.top_color = None;
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

        let mut cursor = -ends.0;
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
        piece(self, cursor, length + ends.1, 0.0, height);
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
        // Wooden parts carry the grain direction: along the piece's width, as an angle in
        // [0, π) packed into the material number (see `worktop` in the shader).
        let grain = item.rotation.rem_euclid(std::f32::consts::PI) / std::f32::consts::PI;
        for part in parts {
            let mut local = LocalMesh::default();
            local.add_part(part, 28);
            self.material = if part.wood { 2.0 + grain * 0.9 } else { 0.0 };
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
        self.material = 0.0;
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

// --- 5. CAMERA ---

/// Builds this frame's projection for the viewport from the camera settings.
pub fn update_projection(app: &mut RoomPlannerApp, rect: egui::Rect) {
    let aspect = rect.width() / rect.height().max(1.0);
    app.view3d_aspect = aspect;
    let (view_proj, eye) = match app.camera_mode {
        CameraMode::Top => {
            // Orthographic, straight down, matching the plan's pan and zoom exactly:
            // screen = plan * zoom + pan_offset.
            let (hw, hh) = (rect.width() / 2.0, rect.height().max(1.0) / 2.0);
            let k = PIXELS_PER_METER * app.zoom_factor;
            let (depth_top, depth_bottom) = (20.0, -1.0);
            let span = depth_top - depth_bottom;
            let m = Mat4::from_cols(
                glam::Vec4::new(k / hw, 0.0, 0.0, 0.0),
                glam::Vec4::new(0.0, 0.0, -1.0 / span, 0.0),
                glam::Vec4::new(0.0, -k / hh, 0.0, 0.0),
                glam::Vec4::new(
                    (app.pan_offset.x - rect.center().x) / hw,
                    -(app.pan_offset.y - rect.center().y) / hh,
                    depth_top / span,
                    1.0,
                ),
            );
            let center = (rect.center().to_vec2() - app.pan_offset) / k;
            (m, Vec3::new(center.x, 50.0, center.y))
        }
        CameraMode::Orbit => {
            let target = orbit_target(app);
            let offset = Vec3::new(
                app.orbit_pitch.cos() * app.orbit_yaw.cos(),
                app.orbit_pitch.sin(),
                app.orbit_pitch.cos() * app.orbit_yaw.sin(),
            ) * app.orbit_distance;
            let eye = target + offset;
            let view = Mat4::look_at_rh(eye, target, Vec3::Y);
            let proj = Mat4::perspective_rh(50.0_f32.to_radians(), aspect, 0.05, 300.0);
            (proj * view, eye)
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
            let view = Mat4::look_at_rh(eye, eye + dir, Vec3::Y);
            let proj = Mat4::perspective_rh(65.0_f32.to_radians(), aspect, 0.05, 300.0);
            (proj * view, eye)
        }
    };
    app.projection = Projection {
        rect,
        view_proj,
        inverse: view_proj.inverse(),
        eye,
    };
}

fn orbit_target(app: &RoomPlannerApp) -> Vec3 {
    Vec3::new(app.orbit_pan.x, 0.8, app.orbit_pan.y)
}

/// Mouse and keyboard navigation for the current camera. Returns true while the pointer
/// is used to move the camera, so editing tools leave it alone.
pub fn handle_camera_input(
    app: &mut RoomPlannerApp,
    ui: &egui::Ui,
    response: &egui::Response,
) -> bool {
    let ctx = ui.ctx();
    let typing = ctx.egui_wants_keyboard_input();
    let drag = response.drag_delta();
    let (scroll, zoom, shift, space) = if response.contains_pointer() {
        ctx.input(|i| {
            (
                i.smooth_scroll_delta,
                i.zoom_delta(),
                i.modifiers.shift,
                !typing && i.key_down(egui::Key::Space),
            )
        })
    } else {
        (egui::Vec2::ZERO, 1.0, false, false)
    };
    // Trackpads report smooth scrolling in points, mouse wheels in lines. Remember the last
    // one seen so the eased tail of a wheel scroll keeps its meaning.
    // Raw trackpad movement too: egui turns Shift+scroll into sideways-only scrolling.
    let mut raw_scroll = egui::Vec2::ZERO;
    if response.contains_pointer() {
        ctx.input(|i| {
            for event in &i.events {
                if let egui::Event::MouseWheel { unit, delta, .. } = event {
                    app.trackpad_scroll = *unit == egui::MouseWheelUnit::Point;
                    if app.trackpad_scroll {
                        raw_scroll += *delta;
                    }
                }
            }
        });
    }
    let trackpad = app.trackpad_scroll;
    let right = response.dragged_by(egui::PointerButton::Secondary);
    let middle = response.dragged_by(egui::PointerButton::Middle);
    let left = response.dragged_by(egui::PointerButton::Primary);

    let navigating = match app.camera_mode {
        CameraMode::Top => {
            let mut zoom = zoom;
            // Wheel zooms; two-finger scroll pans (pinch zooms) unless asked otherwise.
            if !trackpad || app.scroll_zooms {
                zoom *= 1.0 + scroll.y * 0.002;
            } else {
                app.pan_offset += scroll;
            }
            if zoom != 1.0 {
                if let Some(mouse_pos) = response.hover_pos() {
                    app.zoom_at(mouse_pos, zoom);
                }
            }
            let panning = right || middle || (left && space);
            if panning {
                app.pan_offset += drag;
            }
            panning
        }
        CameraMode::Orbit => {
            let pan = middle || (right && shift) || (left && space && shift);
            let orbit = !pan && (right || (left && space));
            if pan {
                // Move the target along the ground so the floor follows the mouse.
                let moved = app.world_drag_delta(response) / PIXELS_PER_METER;
                app.orbit_pan -= moved;
            } else if orbit {
                app.orbit_yaw += drag.x * 0.008;
                app.orbit_pitch = (app.orbit_pitch + drag.y * 0.006).clamp(0.08, 1.55);
            }
            let mut wheel = 0.0;
            if trackpad {
                // Two fingers rotate the view, with Shift they slide it; pinch zooms.
                if shift {
                    // Slide the floor with the fingers, like a drag.
                    let at = response.hover_pos().unwrap_or(response.rect.center());
                    let moved = app.screen_to_world(at + raw_scroll) - app.screen_to_world(at);
                    app.orbit_pan -= moved / PIXELS_PER_METER;
                } else {
                    app.orbit_yaw += scroll.x * 0.006;
                    app.orbit_pitch = (app.orbit_pitch + scroll.y * 0.005).clamp(0.08, 1.55);
                }
            } else {
                wheel = scroll.y;
            }
            if wheel != 0.0 || zoom != 1.0 {
                app.orbit_distance =
                    (app.orbit_distance * (-wheel * 0.002).exp() / zoom).clamp(1.0, 80.0);
            }
            pan || orbit
        }
        CameraMode::Walk => {
            let dir = egui::vec2(app.camera_angle.cos(), app.camera_angle.sin());
            if right || middle {
                // Step: drag up to walk forward, sideways to step aside.
                let side = egui::vec2(-dir.y, dir.x);
                app.camera_pos += (side * drag.x - dir * drag.y) * 0.012 * PIXELS_PER_METER;
            } else if left {
                app.camera_angle += drag.x * 0.005;
                app.camera_pitch = (app.camera_pitch - drag.y * 0.005).clamp(-1.3, 1.3);
            }
            app.camera_pos += dir * scroll.y * 0.008 * PIXELS_PER_METER;
            if trackpad {
                app.camera_angle -= scroll.x * 0.004;
            }

            if !typing {
                let (dt, keys) = ctx.input(|i| {
                    (
                        i.stable_dt.min(0.1),
                        [
                            i.key_down(egui::Key::ArrowUp) || i.key_down(egui::Key::W),
                            i.key_down(egui::Key::ArrowDown) || i.key_down(egui::Key::S),
                            i.key_down(egui::Key::ArrowLeft) || i.key_down(egui::Key::A),
                            i.key_down(egui::Key::ArrowRight) || i.key_down(egui::Key::D),
                        ],
                    )
                });
                let [up, down, turn_left, turn_right] = keys;
                let speed = 1.8 * PIXELS_PER_METER * dt;
                if up {
                    app.camera_pos += dir * speed;
                }
                if down {
                    app.camera_pos -= dir * speed;
                }
                if turn_left {
                    app.camera_angle -= 1.8 * dt;
                }
                if turn_right {
                    app.camera_angle += 1.8 * dt;
                }
                if keys.iter().any(|&k| k) {
                    ctx.request_repaint();
                }
            }
            response.dragged()
        }
    };
    if navigating {
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    navigating
}

/// Where to stand when walking starts: open floor inside a room, near `near`, clear of walls
/// and furniture. Returns the spot and a direction to look in (radians, like
/// `camera_angle`): `facing` if there is room to see that way, otherwise the most open one.
pub fn walk_start(app: &RoomPlannerApp, near: egui::Pos2, facing: f32) -> (egui::Pos2, f32) {
    use crate::geometry::math::{closest_point_on_segment, ray_segment_distance};
    // Things that block you: walls, and furniture you can't see over or step on.
    let mut segments: Vec<(egui::Pos2, egui::Pos2)> =
        app.walls.iter().map(|w| (w.start, w.end)).collect();
    let mut footprints = Vec::new();
    for item in &app.furniture {
        if item.elevation < 1.5 && item.elevation + item.height > 0.15 {
            let corners = item.footprint();
            for k in 0..corners.len() {
                segments.push((corners[k], corners[(k + 1) % corners.len()]));
            }
            footprints.push(corners);
        }
    }
    let clearance = |p: egui::Pos2| {
        if footprints.iter().any(|f| point_in_polygon(p, f)) {
            return 0.0;
        }
        segments
            .iter()
            .map(|&(a, b)| closest_point_on_segment(p, a, b).distance(p))
            .fold(f32::INFINITY, f32::min)
            / PIXELS_PER_METER
    };

    let wanted = 0.7; // meters of space around you
    let mut best = (f32::NEG_INFINITY, near);
    let step = 0.25 * PIXELS_PER_METER;
    for room in &app.rooms {
        let mut bounds = egui::Rect::NOTHING;
        for &p in room {
            bounds.extend_with(p);
        }
        let mut y = bounds.min.y + step / 2.0;
        while y < bounds.max.y {
            let mut x = bounds.min.x + step / 2.0;
            while x < bounds.max.x {
                let p = egui::pos2(x, y);
                if point_in_polygon(p, room) {
                    // Enough space matters most; after that, stay close to where you looked.
                    let score =
                        clearance(p).min(wanted) * 10.0 - p.distance(near) / PIXELS_PER_METER;
                    if score > best.0 {
                        best = (score, p);
                    }
                }
                x += step;
            }
            y += step;
        }
    }
    let spot = best.1;

    // How far you can see along the floor in a direction, in meters.
    let view = |angle: f32| {
        let dir = egui::vec2(angle.cos(), angle.sin());
        segments
            .iter()
            .filter_map(|&(a, b)| ray_segment_distance(spot, dir, a, b))
            .fold(20.0 * PIXELS_PER_METER, f32::min)
            / PIXELS_PER_METER
    };
    // Averaged across the field of view, so a room beats a narrow doorway.
    let open = |angle: f32| {
        (-6..=6)
            .map(|k| view(angle + (k as f32 * 5.0).to_radians()))
            .sum::<f32>()
            / 13.0
    };
    let angle = if open(facing) >= 2.0 {
        facing
    } else {
        (0..32)
            .map(|k| k as f32 / 32.0 * std::f32::consts::TAU)
            .max_by(|&a, &b| open(a).total_cmp(&open(b)))
            .unwrap_or(facing)
    };
    (spot, angle)
}

fn point_in_polygon(p: egui::Pos2, polygon: &[egui::Pos2]) -> bool {
    let mut inside = false;
    let mut j = polygon.len().saturating_sub(1);
    for i in 0..polygon.len() {
        let (a, b) = (polygon[i], polygon[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Center of everything in the plan, in 3D meters.
pub fn plan_center(app: &RoomPlannerApp) -> Vec3 {
    let bounds = plan_bounds(app);
    let c = if bounds.is_positive() {
        bounds.center()
    } else {
        egui::Pos2::ZERO
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

/// Sets the overview camera so the whole plan is in view.
pub fn fit_orbit(app: &mut RoomPlannerApp) {
    let center = plan_center(app);
    app.orbit_pan = egui::vec2(center.x, center.z);
    let bounds = plan_bounds(app);
    if bounds.is_positive() {
        let extent = bounds.size().max_elem() / PIXELS_PER_METER;
        // A tall, narrow view needs to back off further to fit the plan's width.
        let narrow = (1.0 / app.view3d_aspect.max(0.1)).max(1.0);
        app.orbit_distance = (extent * 1.25 * narrow + 2.0).clamp(1.5, 80.0);
    }
}

/// Height walls are drawn at with the current camera.
pub fn shown_wall_height(app: &RoomPlannerApp, wall: &Wall) -> f32 {
    if app.camera_mode == CameraMode::Orbit && app.cut_walls {
        wall.height.min(CUT_WALL_HEIGHT)
    } else {
        wall.height
    }
}

/// The plan position of whatever is under a screen point: a wall or piece of furniture if
/// the ray hits one, otherwise the floor. In the top view this is just the floor point.
pub fn pick(app: &RoomPlannerApp, screen: egui::Pos2) -> egui::Pos2 {
    pick_with_furniture(app, screen).0
}

/// Height in meters of the point under a screen position: on the wall or piece the ray
/// hits, or 0 on the floor.
pub fn pick_height(app: &RoomPlannerApp, screen: egui::Pos2) -> f32 {
    pick_with_furniture(app, screen).2
}

/// Like [`pick`], and also the piece of furniture when that is what the ray hits first.
/// Needed because a piece standing against a wall is within reach of that wall on the plan.
pub fn pick_with_furniture(
    app: &RoomPlannerApp,
    screen: egui::Pos2,
) -> (egui::Pos2, Option<usize>, f32) {
    let floor = app.screen_to_world(screen);
    if app.camera_mode == CameraMode::Top {
        return (floor, None, 0.0);
    }
    let mut hit_furniture = None;
    let (o, d) = app.projection.ray(screen);
    let mut best = if d.y < -1e-4 {
        -o.y / d.y
    } else {
        f32::INFINITY
    };
    let mut hit_point = None;

    for wall in &app.walls {
        let a = egui::vec2(wall.start.x, wall.start.y) / PIXELS_PER_METER;
        let b = egui::vec2(wall.end.x, wall.end.y) / PIXELS_PER_METER;
        let length = (b - a).length();
        if length < 1e-4 {
            continue;
        }
        let dir = (b - a) / length;
        let n = dir.rot90();
        let h = shown_wall_height(app, wall);
        let half = WALL_THICKNESS / 2.0;
        // The two faces and the top.
        let dn = n.x * d.x + n.y * d.z;
        for side in [-half, half] {
            if dn.abs() < 1e-6 {
                continue;
            }
            let rel = a + n * side - egui::vec2(o.x, o.z);
            let t = (rel.x * n.x + rel.y * n.y) / dn;
            if t <= 0.0 || t >= best {
                continue;
            }
            let p = o + d * t;
            let u = (egui::vec2(p.x, p.z) - a).dot(dir);
            if (-half..=length + half).contains(&u) && (0.0..=h).contains(&p.y) {
                best = t;
                hit_point = Some(a + dir * u);
            }
        }
        if d.y.abs() > 1e-6 {
            let t = (h - o.y) / d.y;
            if t > 0.0 && t < best {
                let p = o + d * t;
                let rel = egui::vec2(p.x, p.z) - a;
                let u = rel.dot(dir);
                if (-half..=length + half).contains(&u) && rel.dot(n).abs() <= half {
                    best = t;
                    hit_point = Some(a + dir * u);
                }
            }
        }
    }

    for (index, item) in app.furniture.iter().enumerate() {
        // Slab test in the piece's own frame.
        let origin = item.world_to_local(egui::pos2(o.x, o.z) * PIXELS_PER_METER);
        let rot = egui::emath::Rot2::from_angle(-item.rotation);
        let dir = rot * egui::vec2(d.x, d.z);
        // Thin things like a bar light or a strip get a slightly larger target, so they
        // can be pointed at.
        const MIN_TARGET: f32 = 0.12;
        let (hw, hh, hd) = (
            item.width.max(MIN_TARGET) / 2.0,
            item.height.max(MIN_TARGET) / 2.0,
            item.depth.max(MIN_TARGET) / 2.0,
        );
        let mid = item.elevation + item.height / 2.0;
        let lo = Vec3::new(-hw, mid - hh, -hd);
        let hi = Vec3::new(hw, mid + hh, hd);
        let (ro, rd) = (
            Vec3::new(origin.x, o.y, origin.y),
            Vec3::new(dir.x, d.y, dir.y),
        );
        let mut t0: f32 = 0.0;
        let mut t1 = best;
        for axis in 0..3 {
            if rd[axis].abs() < 1e-8 {
                if ro[axis] < lo[axis] || ro[axis] > hi[axis] {
                    t1 = -1.0;
                }
                continue;
            }
            let (mut ta, mut tb) = (
                (lo[axis] - ro[axis]) / rd[axis],
                (hi[axis] - ro[axis]) / rd[axis],
            );
            if ta > tb {
                std::mem::swap(&mut ta, &mut tb);
            }
            t0 = t0.max(ta);
            t1 = t1.min(tb);
        }
        if t0 <= t1 && t1 > 0.0 && t0 < best {
            best = t0;
            // Aim at the piece's middle so it is found even when hit on its edge.
            let p = o + d * t0;
            let inside = item.world_to_local(egui::pos2(p.x, p.z) * PIXELS_PER_METER);
            let clamped = egui::vec2(
                inside.x.clamp(-item.width * 0.45, item.width * 0.45),
                inside.y.clamp(-item.depth * 0.45, item.depth * 0.45),
            );
            let w = item.local_to_world(clamped.x, clamped.y);
            hit_point = Some(egui::vec2(w.x, w.y) / PIXELS_PER_METER);
            hit_furniture = Some(index);
        }
    }

    match hit_point {
        Some(p) => (
            egui::pos2(p.x, p.y) * PIXELS_PER_METER,
            hit_furniture,
            (o + d * best).y.max(0.0),
        ),
        None => (floor, None, 0.0),
    }
}

// --- 6. DRAWING ---

/// Renders the scene into `rect`. `preview` is furniture being dragged in from the library;
/// `hovered_wall` gets a highlight like the selected walls.
pub fn paint(
    app: &mut RoomPlannerApp,
    painter: &egui::Painter,
    rect: egui::Rect,
    preview: Option<&Furniture>,
    hovered_wall: Option<usize>,
) {
    let env = lighting(app.time_of_day);
    painter.rect_filled(rect, 0.0, env.sky);
    let eye = app.projection.eye;

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
    // The ceiling sits on the tallest wall.
    let ceiling = app
        .walls
        .iter()
        .map(|w| w.height)
        .fold(None, |m: Option<f32>, h| Some(m.map_or(h, |m| m.max(h))))
        .unwrap_or(WALL_HEIGHT);
    for room in &app.rooms {
        mesh.material = if app.finishes.floor.is_wood() {
            1.0
        } else {
            0.0
        };
        mesh.polygon(room, 0.0, Vec3::Y, srgb(app.finishes.floor.color()));
        mesh.material = 0.0;
        if walk {
            mesh.polygon(room, ceiling, -Vec3::Y, [0.96, 0.96, 0.95]);
        }
    }

    // From above, wall tops are drawn dark like the cut walls on a floor plan.
    let dark_tops = match app.camera_mode {
        CameraMode::Top => true,
        CameraMode::Orbit => app.cut_walls,
        CameraMode::Walk => false,
    };
    let base = srgb(app.finishes.wall_color);
    for (idx, wall) in app.walls.iter().enumerate() {
        let tint = if app.selected_walls.contains(&idx) {
            Some(([0.15, 0.39, 0.92], 0.5))
        } else if hovered_wall == Some(idx) {
            Some(([0.47, 0.63, 0.96], 0.35))
        } else {
            None
        };
        let mix = |c: [f32; 3]| match tint {
            Some((t, k)) => [0, 1, 2].map(|i| c[i] * (1.0 - k) + t[i] * k),
            None => c,
        };
        let top = dark_tops.then(|| mix([0.30, 0.32, 0.37]));
        let ends = crate::geometry::walls::wall_end_extensions(&app.walls, idx);
        // A highlighted wall sits a hair higher, so its top wins where walls of different
        // heights or odd angles still overlap.
        let height = shown_wall_height(app, wall) + if tint.is_some() { 0.002 } else { 0.0 };
        mesh.wall(wall, ends, height, mix(base), top);
    }

    let mut lights = Vec::new();
    // Index ranges of lamps, which don't cast shadows.
    let mut no_shadow: Vec<std::ops::Range<usize>> = Vec::new();
    for item in app.furniture.iter().chain(preview) {
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
        let ceiling = crate::geometry::walls::ceiling_height_at(&app.walls, item.position);
        let mut parts = item.parts_under(ceiling);
        for part in &mut parts {
            // Lift parts on the floor a hair so rugs don't flicker against it.
            part.min[1] = part.min[1].max(0.002);
        }
        let first = mesh.indices.len();
        mesh.furniture(item, &parts);
        if item.is_lamp() {
            no_shadow.push(first..mesh.indices.len());
        }
        for light in lamp::lights(item) {
            let pos = local_to_3d(item, Vec3::from(light.local));
            let beam = light
                .beam
                .map(|(dir, cos)| (local_normal_to_3d(item, Vec3::from(dir)), cos));
            lights.push((pos, light, beam));
        }
    }

    // The shader takes a fixed number of lights; keep the ones closest to the camera.
    lights.sort_by(|a, b| {
        a.0.distance_squared(eye)
            .total_cmp(&b.0.distance_squared(eye))
    });
    let mut scene = SceneUniform {
        view_proj: app.projection.view_proj.to_cols_array_2d(),
        camera_pos: [
            eye.x,
            eye.y,
            eye.z,
            // No grid lines on the floor at eye height.
            if app.show_grid && app.camera_mode != CameraMode::Walk {
                1.0
            } else {
                0.0
            },
        ],
        ambient: [
            env.ambient[0],
            env.ambient[1],
            env.ambient[2],
            lights.len().min(MAX_LIGHTS) as f32,
        ],
        sun: [0.35, 0.8, 0.45, env.sun],
        shadow_mats: [[[0.0; 4]; 4]; MAX_SHADOWS],
        lights: [GpuLight {
            position: [0.0; 4],
            color: [0.0; 4],
            direction: [0.0, -1.0, 0.0, -2.0],
            shadow: [-1.0, 0.0, 0.0, 0.0],
        }; MAX_LIGHTS],
    };
    let mut shadow_count = 0;
    for (slot, (pos, light, beam)) in scene.lights.iter_mut().zip(&lights) {
        slot.position = [pos.x, pos.y, pos.z, 1.0];
        let c = light.color.map(|c| c * env.lamps);
        slot.color = [c[0], c[1], c[2], light.range];
        slot.direction = match beam {
            Some((dir, cos)) => [dir.x, dir.y, dir.z, *cos],
            None => [0.0, -1.0, 0.0, -2.0],
        };
        // The nearest lights get shadow maps, looking along their beam (or down).
        if app.shadows && shadow_count < MAX_SHADOWS {
            scene.shadow_mats[shadow_count] =
                shadow_view(*pos, *beam, light.range).to_cols_array_2d();
            slot.shadow = [shadow_count as f32, 0.0, 0.0, 0.0];
            shadow_count += 1;
        }
    }
    let caster_indices: Vec<u32> = if shadow_count == 0 {
        Vec::new()
    } else {
        let mut skip = no_shadow.iter().peekable();
        mesh.indices
            .chunks(3)
            .enumerate()
            .filter(|(t, _)| {
                let at = t * 3;
                while skip.peek().is_some_and(|r| r.end <= at) {
                    skip.next();
                }
                !skip.peek().is_some_and(|r| r.contains(&at))
            })
            .flat_map(|(_, tri)| tri.iter().copied())
            .collect()
    };

    let callback = egui_wgpu::Callback::new_paint_callback(
        rect,
        True3dCallback {
            vertices: mesh.vertices,
            indices: mesh.indices,
            caster_indices,
            shadow_count,
            scene,
        },
    );
    painter.add(callback);
}

/// The view of a light's shadow map: from the light along its beam, or straight down for
/// lamps that shine all round, wide enough to cover the beam.
fn shadow_view(pos: Vec3, beam: Option<(Vec3, f32)>, range: f32) -> Mat4 {
    let (dir, fov) = match beam {
        Some((dir, cos)) => (dir, 2.0 * cos.clamp(-1.0, 1.0).acos() + 0.35),
        None => (Vec3::NEG_Y, 2.44),
    };
    let fov = fov.clamp(0.5, 2.44); // up to 140°
    let dir = dir.normalize_or(Vec3::NEG_Y);
    let up = if dir.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
    let view = Mat4::look_at_rh(pos, pos + dir, up);
    Mat4::perspective_rh(fov, 1.0, 0.03, range.max(0.5)) * view
}

#[cfg(test)]
mod walk_tests {
    use super::*;

    #[test]
    fn walking_starts_in_a_room_clear_of_walls_and_furniture() {
        let mut app = RoomPlannerApp::default();
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("example_plans/two_bedroom.json");
        crate::storage::apply_state(&mut app, crate::storage::read_state(&path).unwrap());
        // Looking from outside the plan and from inside a wall both end up on open floor.
        for near in [egui::pos2(600.0, 200.0), egui::pos2(250.0, 100.0)] {
            let (spot, _) = walk_start(&app, near, 0.0);
            assert!(app.rooms.iter().any(|r| point_in_polygon(spot, r)));
            for w in &app.walls {
                let d = crate::geometry::math::closest_point_on_segment(spot, w.start, w.end)
                    .distance(spot);
                assert!(d > 0.5 * PIXELS_PER_METER, "{spot:?} is {d} from a wall");
            }
            for item in &app.furniture {
                if item.height > 0.15 {
                    assert!(
                        !point_in_polygon(spot, &item.footprint()),
                        "inside {}",
                        item.name
                    );
                }
            }
        }
    }
}
