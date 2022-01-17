//! Panel renderer

// Modules
mod uniform;
mod vertex;

// Exports
pub use uniform::PanelUniforms;
pub use vertex::PanelVertex;

// Imports
use crate::Panel;
use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;

/// Renderer for all panels
///
/// Responsible for rendering all panels.
pub struct PanelsRenderer {
	/// Render pipeline
	render_pipeline: wgpu::RenderPipeline,

	/// Index buffer
	///
	/// Since we're just rendering rectangles, the indices
	/// buffer is shared for all panels for now.
	indices: wgpu::Buffer,

	/// Uniform bind group
	uniforms_bind_group_layout: wgpu::BindGroupLayout,

	/// Image bind group layout
	texture_bind_group_layout: wgpu::BindGroupLayout,
}

impl PanelsRenderer {
	/// Creates a new renderer for the panels
	#[allow(clippy::too_many_lines)] // TODO:
	pub async fn new(device: &wgpu::Device, texture_format: wgpu::TextureFormat) -> Result<Self, anyhow::Error> {
		// Create the index buffer
		const INDICES: [u32; 6] = [0, 1, 3, 0, 3, 2];
		let index_buffer_descriptor = wgpu::util::BufferInitDescriptor {
			label:    Some("Index buffer"),
			contents: bytemuck::cast_slice(&INDICES),
			usage:    wgpu::BufferUsages::INDEX,
		};
		let indices = device.create_buffer_init(&index_buffer_descriptor);

		// Create the image bind group layout
		let texture_bind_group_layout_descriptor = wgpu::BindGroupLayoutDescriptor {
			entries: &[
				wgpu::BindGroupLayoutEntry {
					binding:    0,
					visibility: wgpu::ShaderStages::FRAGMENT,
					ty:         wgpu::BindingType::Texture {
						multisampled:   false,
						view_dimension: wgpu::TextureViewDimension::D2,
						sample_type:    wgpu::TextureSampleType::Float { filterable: true },
					},
					count:      None,
				},
				wgpu::BindGroupLayoutEntry {
					binding:    1,
					visibility: wgpu::ShaderStages::FRAGMENT,
					ty:         wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
					count:      None,
				},
			],
			label:   None,
		};
		let texture_bind_group_layout = device.create_bind_group_layout(&texture_bind_group_layout_descriptor);

		// Create the uniform bind group
		let uniforms_bind_group_layout_descriptor = wgpu::BindGroupLayoutDescriptor {
			entries: &[wgpu::BindGroupLayoutEntry {
				binding:    0,
				visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
				ty:         wgpu::BindingType::Buffer {
					ty:                 wgpu::BufferBindingType::Uniform,
					has_dynamic_offset: false,
					min_binding_size:   None,
				},
				count:      None,
			}],
			label:   None,
		};
		let uniforms_bind_group_layout = device.create_bind_group_layout(&uniforms_bind_group_layout_descriptor);

		// Load the shader
		let shader_descriptor = wgpu::ShaderModuleDescriptor {
			label:  Some("Shader"),
			source: wgpu::ShaderSource::Wgsl(include_str!("renderer/shader.wgsl").into()),
		};
		let shader = device.create_shader_module(&shader_descriptor);

		// Create the pipeline layout
		let render_pipeline_layout_descriptor = wgpu::PipelineLayoutDescriptor {
			label:                Some("Render pipeline layout"),
			bind_group_layouts:   &[&uniforms_bind_group_layout, &texture_bind_group_layout],
			push_constant_ranges: &[],
		};
		let render_pipeline_layout = device.create_pipeline_layout(&render_pipeline_layout_descriptor);

		// Create the render pipeline
		let color_targets = [wgpu::ColorTargetState {
			format:     texture_format,
			blend:      Some({
				/* wgpu::BlendState {
					color: wgpu::BlendComponent {
						src_factor: wgpu::BlendFactor::Src,
						dst_factor: wgpu::BlendFactor::Dst,
						operation: wgpu::BlendOperation::Add,
					},
					alpha: wgpu::BlendComponent::OVER,
				} */
				let blend = std::fs::read("blend.json").expect("");
				serde_json::from_slice(&blend).expect("")
			}),
			write_mask: wgpu::ColorWrites::ALL,
		}];
		let render_pipeline_descriptor = wgpu::RenderPipelineDescriptor {
			label:         Some("Render pipeline"),
			layout:        Some(&render_pipeline_layout),
			vertex:        wgpu::VertexState {
				module:      &shader,
				entry_point: "vs_main",
				buffers:     &[PanelVertex::buffer_layout()],
			},
			primitive:     wgpu::PrimitiveState {
				topology:           wgpu::PrimitiveTopology::TriangleList,
				strip_index_format: None,
				front_face:         wgpu::FrontFace::Ccw,
				cull_mode:          None,
				unclipped_depth:    false,
				polygon_mode:       wgpu::PolygonMode::Fill,
				conservative:       false,
			},
			depth_stencil: None,
			multisample:   wgpu::MultisampleState {
				count: 1,
				mask: u64::MAX,
				alpha_to_coverage_enabled: false,
			},
			fragment:      Some(wgpu::FragmentState {
				module:      &shader,
				entry_point: "fs_main",
				targets:     &color_targets,
			}),
			multiview:     None,
		};
		let render_pipeline = device.create_render_pipeline(&render_pipeline_descriptor);

		Ok(Self {
			render_pipeline,
			indices,
			uniforms_bind_group_layout,
			texture_bind_group_layout,
		})
	}

	/// Returns the bind group layout
	pub const fn uniforms_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
		&self.uniforms_bind_group_layout
	}

	/// Returns the texture bind group layout
	pub const fn texture_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
		&self.texture_bind_group_layout
	}

	/// Renders all panels
	pub fn render(
		&self, panels: &mut [Panel], encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, queue: &wgpu::Queue,
		window_size: PhysicalSize<u32>,
	) -> Result<(), anyhow::Error> {
		// Update the uniforms
		// TODO:

		let render_pass_descriptor = wgpu::RenderPassDescriptor {
			label:                    Some("Render pass"),
			color_attachments:        &[wgpu::RenderPassColorAttachment {
				view,
				resolve_target: None,
				ops: wgpu::Operations {
					load:  wgpu::LoadOp::Clear(wgpu::Color {
						r: 0.0,
						g: 0.0,
						b: 0.0,
						a: 1.0,
					}),
					store: true,
				},
			}],
			depth_stencil_attachment: None,
		};
		let mut render_pass = encoder.begin_render_pass(&render_pass_descriptor);
		render_pass.set_pipeline(&self.render_pipeline);
		render_pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);

		for panel in panels {
			panel.draw(&mut render_pass, queue, window_size);
		}

		Ok(())
	}
}
