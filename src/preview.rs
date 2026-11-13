//! Interactive 3D preview of the selection, drawn on the GPU inside the egui window
//! (wgpu paint callback): textured, with cut-out and translucent materials.

use crate::export::material_image;
use crate::mesh::{MatKey, Mesh};
use crate::registry::Registry;
use eframe::egui;
use eframe::egui_wgpu::{self, CallbackResources, CallbackTrait, ScreenDescriptor};
use eframe::wgpu;
use eframe::wgpu::util::DeviceExt;
use std::collections::HashMap;
use std::sync::Arc;

const SHADER: &str = r#"
struct U { vp: mat4x4<f32> };
@group(0) @binding(0) var<uniform> u: U;
@group(1) @binding(0) var t: texture_2d<f32>;
@group(1) @binding(1) var s: sampler;
struct VO { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) shade: f32 };
@vertex fn vs(@location(0) p: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) shade: f32) -> VO {
    var o: VO;
    o.pos = u.vp * vec4<f32>(p, 1.0);
    o.uv = uv;
    o.shade = shade;
    return o;
}
@fragment fn fs_cut(i: VO) -> @location(0) vec4<f32> {
    let c = textureSample(t, s, i.uv);
    if (c.a < 0.5) { discard; }
    return vec4<f32>(c.rgb * i.shade, 1.0);
}
@fragment fn fs_blend(i: VO) -> @location(0) vec4<f32> {
    let c = textureSample(t, s, i.uv);
    return vec4<f32>(c.rgb * i.shade, c.a);
}
"#;

/// CPU-side geometry, one batch per material
pub struct PreviewMesh {
    pub batches: Vec<Batch>,
    /// half size of the bounding box (for the initial camera distance)
    pub radius: f32,
    pub quads: usize,
}

pub struct Batch {
    verts: Vec<f32>,
    idx: Vec<u32>,
    tex_w: u32,
    tex_h: u32,
    rgba: Vec<u8>,
    translucent: bool,
}

impl PreviewMesh {
    pub fn build(reg: &Registry, mesh: &Mesh) -> PreviewMesh {
        let sel = mesh.sel;
        let c = [(sel.x1 - sel.x0 + 1) as f32 / 2.0, (sel.y0 + sel.y1 + 1) as f32 / 2.0, (sel.z1 - sel.z0 + 1) as f32 / 2.0];
        let radius = (c[0] * c[0] + ((sel.y1 - sel.y0 + 1) as f32 / 2.0).powi(2) + c[2] * c[2]).sqrt().max(4.0);
        let mut by_mat: HashMap<MatKey, (Vec<f32>, Vec<u32>)> = HashMap::new();
        for q in &mesh.quads {
            let p = q.pos;
            let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
            let e2 = [p[3][0] - p[0][0], p[3][1] - p[0][1], p[3][2] - p[0][2]];
            let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if l < 1e-12 {
                continue;
            }
            let n = n.map(|v| v / l);
            // Minecraft-like directional shading
            let shade = n[1] * n[1] * if n[1] > 0.0 { 1.0 } else { 0.5 } + n[2] * n[2] * 0.8 + n[0] * n[0] * 0.6;
            let (v, idx) = by_mat.entry(q.mat).or_default();
            let base = (v.len() / 6) as u32;
            for k in 0..4 {
                v.extend_from_slice(&[p[k][0] - c[0], p[k][1] - c[1], p[k][2] - c[2], q.uv[k][0], 1.0 - q.uv[k][1], shade]);
            }
            idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        let mut batches: Vec<Batch> = by_mat
            .into_iter()
            .map(|(m, (verts, idx))| {
                let img = material_image(reg, &m);
                Batch { verts, idx, tex_w: img.w, tex_h: img.h, rgba: img.rgba, translucent: img.translucent }
            })
            .collect();
        // opaque first, translucent last
        batches.sort_by_key(|b| b.translucent);
        PreviewMesh { batches, radius, quads: mesh.quads.len() }
    }
}

struct GpuBatch {
    vb: wgpu::Buffer,
    ib: wgpu::Buffer,
    count: u32,
    tex: wgpu::BindGroup,
    translucent: bool,
}

pub struct Renderer {
    cut: wgpu::RenderPipeline,
    blend: wgpu::RenderPipeline,
    tex_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
    uniform_bg: wgpu::BindGroup,
    srgb: bool,
    batches: Vec<GpuBatch>,
    generation: u64,
}

pub const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Creates the GPU resources once, at startup
pub fn init(rs: &egui_wgpu::RenderState) {
    let device = &rs.device;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("preview"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
    let u_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("preview-u"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        }],
    });
    let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("preview-t"),
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
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("preview"),
        bind_group_layouts: &[Some(&u_layout), Some(&tex_layout)],
        immediate_size: 0,
    });
    let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32];
    let pipeline = |fs: &str, blend: bool| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("preview"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout { array_stride: 24, step_mode: wgpu::VertexStepMode::Vertex, attributes: &attrs }],
            },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: Some(!blend),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(fs),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: rs.target_format,
                    blend: if blend { Some(wgpu::BlendState::ALPHA_BLENDING) } else { None },
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let cut = pipeline("fs_cut", false);
    let blend = pipeline("fs_blend", true);
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("preview-u"),
        size: 64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("preview-u"),
        layout: &u_layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
    });
    let r = Renderer {
        cut,
        blend,
        tex_layout,
        sampler,
        uniform,
        uniform_bg,
        srgb: rs.target_format.is_srgb(),
        batches: Vec::new(),
        generation: 0,
    };
    rs.renderer.write().callback_resources.insert(r);
}

/// One frame of the preview
pub struct Frame {
    pub view_proj: [f32; 16],
    pub mesh: Option<Arc<PreviewMesh>>,
    pub generation: u64,
}

impl CallbackTrait for Frame {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _sd: &ScreenDescriptor,
        _enc: &mut wgpu::CommandEncoder,
        res: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(r) = res.get_mut::<Renderer>() else { return Vec::new() };
        if r.generation != self.generation {
            r.generation = self.generation;
            r.batches.clear();
            if let Some(m) = &self.mesh {
                for b in &m.batches {
                    let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: None,
                        contents: &b.verts.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>(),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: None,
                        contents: &b.idx.iter().flat_map(|i| i.to_le_bytes()).collect::<Vec<u8>>(),
                        usage: wgpu::BufferUsages::INDEX,
                    });
                    let size = wgpu::Extent3d { width: b.tex_w, height: b.tex_h, depth_or_array_layers: 1 };
                    let format = if r.srgb { wgpu::TextureFormat::Rgba8UnormSrgb } else { wgpu::TextureFormat::Rgba8Unorm };
                    let tex = device.create_texture(&wgpu::TextureDescriptor {
                        label: None,
                        size,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                        &b.rgba,
                        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * b.tex_w), rows_per_image: Some(b.tex_h) },
                        size,
                    );
                    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
                    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: None,
                        layout: &r.tex_layout,
                        entries: &[
                            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&r.sampler) },
                        ],
                    });
                    r.batches.push(GpuBatch { vb, ib, count: b.idx.len() as u32, tex: bg, translucent: b.translucent });
                }
            }
        }
        let bytes: Vec<u8> = self.view_proj.iter().flat_map(|f| f.to_le_bytes()).collect();
        queue.write_buffer(&r.uniform, 0, &bytes);
        Vec::new()
    }

    fn paint(&self, _info: egui::PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, res: &CallbackResources) {
        let Some(r) = res.get::<Renderer>() else { return };
        pass.set_bind_group(0, &r.uniform_bg, &[]);
        for translucent in [false, true] {
            pass.set_pipeline(if translucent { &r.blend } else { &r.cut });
            for b in r.batches.iter().filter(|b| b.translucent == translucent) {
                pass.set_bind_group(1, &b.tex, &[]);
                pass.set_vertex_buffer(0, b.vb.slice(..));
                pass.set_index_buffer(b.ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..b.count, 0, 0..1);
            }
        }
    }
}

/// Orbit camera
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub target: [f32; 3],
}

impl Camera {
    pub fn new(radius: f32) -> Camera {
        Camera { yaw: 0.8, pitch: 0.6, dist: radius * 2.2, target: [0.0; 3] }
    }

    pub fn eye(&self) -> [f32; 3] {
        let (cp, sp) = (self.pitch.cos(), self.pitch.sin());
        [
            self.target[0] + self.dist * cp * self.yaw.cos(),
            self.target[1] + self.dist * sp,
            self.target[2] + self.dist * cp * self.yaw.sin(),
        ]
    }

    /// Moves the target in the view plane (dx, dy in pixels)
    pub fn pan(&mut self, dx: f32, dy: f32, height_px: f32) {
        let e = self.eye();
        let f = norm(sub(self.target, e));
        let r = norm(cross(f, [0.0, 1.0, 0.0]));
        let u = cross(r, f);
        let k = self.dist * 0.9 / height_px.max(1.0);
        for i in 0..3 {
            self.target[i] += (-r[i] * dx + u[i] * dy) * k;
        }
    }

    /// Column-major view-projection matrix (depth 0..1)
    pub fn view_proj(&self, aspect: f32, radius: f32) -> [f32; 16] {
        let e = self.eye();
        let f = norm(sub(self.target, e));
        let s = norm(cross(f, [0.0, 1.0, 0.0]));
        let u = cross(s, f);
        let view = [
            s[0], u[0], -f[0], 0.0,
            s[1], u[1], -f[1], 0.0,
            s[2], u[2], -f[2], 0.0,
            -dot(s, e), -dot(u, e), dot(f, e), 1.0,
        ];
        let near = (self.dist * 0.01).max(0.05);
        let far = self.dist + radius * 4.0 + 100.0;
        let t = 1.0 / (0.45f32).tan();
        let proj = [
            t / aspect, 0.0, 0.0, 0.0,
            0.0, t, 0.0, 0.0,
            0.0, 0.0, far / (near - far), -1.0,
            0.0, 0.0, near * far / (near - far), 0.0,
        ];
        mul(&proj, &view)
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt().max(1e-9);
    a.map(|v| v / l)
}
/// a × b, column-major
fn mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut o = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    o
}

/// Draws the preview in `rect` and handles the camera controls
pub fn show(ui: &mut egui::Ui, rect: egui::Rect, cam: &mut Camera, mesh: &Option<Arc<PreviewMesh>>, generation: u64) {
    let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());
    let radius = mesh.as_ref().map(|m| m.radius).unwrap_or(16.0);
    if resp.dragged_by(egui::PointerButton::Primary) {
        let d = resp.drag_delta();
        cam.yaw += d.x * 0.008;
        cam.pitch = (cam.pitch + d.y * 0.008).clamp(-1.55, 1.55);
    }
    if resp.dragged_by(egui::PointerButton::Secondary) || resp.dragged_by(egui::PointerButton::Middle) {
        let d = resp.drag_delta();
        cam.pan(d.x, d.y, rect.height());
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            cam.dist = (cam.dist * (1.0 - scroll * 0.0015)).clamp(1.0, radius * 20.0 + 50.0);
        }
    }
    if resp.double_clicked() {
        *cam = Camera::new(radius);
    }
    let vp = cam.view_proj(rect.width() / rect.height().max(1.0), radius);
    ui.painter().add(egui_wgpu::Callback::new_paint_callback(rect, Frame { view_proj: vp, mesh: mesh.clone(), generation }));
}
