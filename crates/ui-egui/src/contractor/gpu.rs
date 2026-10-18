//! Offscreen framing renderer: independent depth attachment, displayed as an egui texture.
use cadcraft_contractor::view3d::{Camera, Scene};
use cadcraft_doc::Handle;
use egui_wgpu::wgpu::{self, util::DeviceExt};

pub const SHADER: &str = r#"
struct VertexOut { @builtin(position) position:vec4<f32>, @location(0) color:vec4<f32> }
@vertex fn vs(@location(0) p:vec3<f32>, @location(1) color:vec4<f32>) -> VertexOut {
    var out:VertexOut; out.position=vec4<f32>(p,1.0);out.color=color;return out;
}
@fragment fn fs(v:VertexOut)->@location(0) vec4<f32> { return v.color; }
"#;

pub struct Renderer {
    state: egui_wgpu::RenderState,
    pipeline: wgpu::RenderPipeline,
    output: Option<Output>,
}
struct Output {
    size: (u32, u32),
    color: wgpu::Texture,
    depth: wgpu::Texture,
    id: egui::TextureId,
}
impl Renderer {
    pub fn new(state: &egui_wgpu::RenderState) -> Self {
        let device = &state.device;
        let shader = device
            .create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("Contractor 3D"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Contractor 3D"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let attributes = [
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 12, shader_location: 1 },
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Contractor depth-tested members"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[Some(wgpu::VertexBufferLayout { array_stride: 28, step_mode: wgpu::VertexStepMode::Vertex, attributes: &attributes })],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8Unorm, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        Self { state: state.clone(), pipeline, output: None }
    }
    pub fn render(
        &mut self,
        scene: &Scene,
        camera: &Camera,
        width: u32,
        height: u32,
        selected: &[Handle],
    ) -> cadcraft_contractor::Result<egui::TextureId> {
        camera.validate()?;
        if scene.triangles.len() > 500_000 {
            return Err(cadcraft_contractor::Error::Invalid("3D scene exceeds triangle budget".into()));
        }
        let size = (width.clamp(1, 2048), height.clamp(1, 2048));
        if self.output.as_ref().is_none_or(|o| o.size != size) {
            if let Some(old) = self.output.take() {
                self.state.renderer.write().free_texture(&old.id);
            }
            let make = |format, usage| {
                self.state.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Contractor viewport"),
                    size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
            };
            let color = make(
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            );
            let depth = make(wgpu::TextureFormat::Depth32Float, wgpu::TextureUsages::RENDER_ATTACHMENT);
            let id = self.state.renderer.write().register_native_texture(
                &self.state.device,
                &color.create_view(&Default::default()),
                wgpu::FilterMode::Linear,
            );
            self.output = Some(Output { size, color, depth, id });
        }
        let Some(output) = &self.output else {
            return Err(cadcraft_contractor::Error::Invalid("3D target unavailable".into()));
        };
        let mut bytes = Vec::with_capacity(scene.triangles.len().saturating_mul(84));
        let (_, _, forward) = camera.basis();
        let zrange = scene.triangles.iter().flat_map(|t| t.points).map(|p| (p - camera.center).dot(forward).abs()).fold(1.0, f64::max) * 2.1;
        let selected: std::collections::HashSet<_> = selected.iter().copied().collect();
        for t in &scene.triangles {
            let c = if selected.contains(&t.handle) { [95, 191, 255] } else { t.color };
            for p in t.points {
                let q = camera.project(p, size.0 as usize, size.1 as usize);
                if !q.is_finite() || q.x.abs() > f64::from(f32::MAX) || q.y.abs() > f64::from(f32::MAX) {
                    return Err(cadcraft_contractor::Error::Invalid("invalid 3D projection".into()));
                }
                let vertex = [
                    (q.x / size.0 as f64 * 2.0 - 1.0) as f32,
                    (1.0 - q.y / size.1 as f64 * 2.0) as f32,
                    (0.5 - q.z / zrange) as f32,
                    f32::from(c[0]) / 255.0,
                    f32::from(c[1]) / 255.0,
                    f32::from(c[2]) / 255.0,
                    1.0,
                ];
                for v in vertex {
                    bytes.extend_from_slice(&v.to_ne_bytes());
                }
            }
        }
        // A nonempty buffer is required even for an empty model; draw count stays zero.
        let count = (bytes.len() / 28).min(u32::MAX as usize) as u32;
        if bytes.is_empty() {
            bytes.resize(28, 0);
        }
        let buffer = self.state.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Contractor triangles"),
            contents: &bytes,
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut encoder = self.state.device.create_command_encoder(&Default::default());
        let color = output.color.create_view(&Default::default());
        let depth = output.depth.create_view(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Contractor 3D"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..count, 0..1);
        }
        self.state.queue.submit([encoder.finish()]);
        Ok(output.id)
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        if let Some(output) = &self.output {
            self.state.renderer.write().free_texture(&output.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_contractor::view3d::Triangle;
    use cadcraft_geom::Vec3;

    #[test]
    fn gpu_depth_keeps_the_upper_member_visible_in_either_draw_order() {
        let rs = egui_kittest::wgpu::create_render_state(egui_kittest::wgpu::default_wgpu_setup(), Default::default());
        let mut renderer = Renderer::new(&rs);
        let camera = Camera { center: Vec3::ZERO, yaw: 0.0, pitch: std::f64::consts::FRAC_PI_2, height: 64.0 };
        let triangle = |z, color, handle| Triangle {
            points: [Vec3::new(-20.0, -20.0, z), Vec3::new(20.0, -20.0, z), Vec3::new(0.0, 20.0, z)],
            color,
            handle: Handle(handle),
        };
        let mut scene = Scene { triangles: vec![triangle(0.0, [200, 0, 0], 1), triangle(5.0, [0, 200, 0], 2)] };
        for _ in 0..2 {
            renderer.render(&scene, &camera, 64, 64, &[]).unwrap();
            let output = renderer.output.as_ref().unwrap();
            let buffer = rs.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 64 * 256,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder = rs.device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_buffer(
                output.color.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256), rows_per_image: Some(64) },
                },
                wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 },
            );
            rs.queue.submit([encoder.finish()]);
            buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            rs.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(std::time::Duration::from_secs(10)) }).unwrap();
            let pixels = buffer.slice(..).get_mapped_range().unwrap();
            assert_eq!(pixels.get(32 * 256 + 32 * 4..32 * 256 + 32 * 4 + 4), Some([0, 200, 0, 255].as_slice()));
            scene.triangles.reverse();
        }
    }
}
