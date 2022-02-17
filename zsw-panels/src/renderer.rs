//! Panels renderer

// Modules
mod uniform;
mod vertex;

// Exports
pub use self::{uniform::PanelUniforms, vertex::PanelVertex};

// Imports
use {crate::PanelState, wgpu::util::DeviceExt, winit::dpi::PhysicalSize};

/// Panels renderer
///
/// Responsible for rendering all panels.
///
/// Exists because all panels share a lot of state, such as
/// their vertices and indices. Using this renderer means each
/// panel instance only needs to store their own uniform buffer
// Note: Vertices and indices are shared because all panels are
//       rendered as just a quad. Their position is determined by
//       the matrix sent in the uniform. Their UVs are also determined
//       via the uniforms.
#[derive(Debug)]
pub struct PanelsRenderer {
	/// Render pipeline
	render_pipeline: wgpu::RenderPipeline,

	/// Vertex buffer
	vertices: wgpu::Buffer,

	/// Index buffer
	indices: wgpu::Buffer,

	/// Uniforms bind group layout
	uniforms_bind_group_layout: wgpu::BindGroupLayout,

	/// Image bind group layout
	image_bind_group_layout: wgpu::BindGroupLayout,
}

impl PanelsRenderer {
	/// Creates a new renderer for the panels
	pub fn new(device: &wgpu::Device, surface_texture_format: wgpu::TextureFormat) -> Result<Self, anyhow::Error> {
		// Create the index buffer
		let indices = self::create_indices(device);

		// Create the vertex buffer
		let vertices = self::create_vertices(device);

		// Create the bind group layouts
		let uniforms_bind_group_layout = self::create_uniforms_bind_group_layout(device);
		let image_bind_group_layout = self::create_image_bind_group_layout(device);

		// Create the render pipeline
		let render_pipeline = self::create_render_pipeline(
			device,
			surface_texture_format,
			&uniforms_bind_group_layout,
			&image_bind_group_layout,
		);

		Ok(Self {
			render_pipeline,
			vertices,
			indices,
			uniforms_bind_group_layout,
			image_bind_group_layout,
		})
	}

	/// Returns the uniforms' bind group layout
	pub fn uniforms_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
		&self.uniforms_bind_group_layout
	}

	/// Returns the image bind group layout
	pub fn image_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
		&self.image_bind_group_layout
	}

	/// Renders panels
	pub fn render<'a>(
		&self,
		panels: impl IntoIterator<Item = &'a PanelState>,
		queue: &wgpu::Queue,
		encoder: &mut wgpu::CommandEncoder,
		surface_view: &wgpu::TextureView,
		surface_size: PhysicalSize<u32>,
	) -> Result<(), anyhow::Error> {
		// Create the render pass for all panels
		let render_pass_descriptor = wgpu::RenderPassDescriptor {
			label:                    Some("[zsw::panel] Render pass"),
			color_attachments:        &[wgpu::RenderPassColorAttachment {
				view:           surface_view,
				resolve_target: None,
				ops:            wgpu::Operations {
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

		// Set our shared pipeline, indices, vertices and uniform bind group
		render_pass.set_pipeline(&self.render_pipeline);
		render_pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
		render_pass.set_vertex_buffer(0, self.vertices.slice(..));

		// And draw each panel
		for panel in panels {
			// Calculate the position matrix for the panel
			let pos_matrix = panel.pos_matrix(surface_size);

			// Then go through all image descriptors to render
			for descriptor in panel.image_descriptors() {
				// Update the uniforms
				let uvs_matrix = descriptor.uvs_matrix();
				let uniforms = PanelUniforms::new(pos_matrix, uvs_matrix, descriptor.alpha());
				let image = descriptor.image();
				queue.write_buffer(image.uniforms(), 0, bytemuck::cast_slice(&[uniforms]));

				// Bind the image and draw
				render_pass.set_bind_group(0, image.uniforms_bind_group(), &[]);
				render_pass.set_bind_group(1, image.image_bind_group(), &[]);
				render_pass.draw_indexed(0..6, 0, 0..1);
			}
		}

		Ok(())
	}
}

/// Creates the vertices
fn create_vertices(device: &wgpu::Device) -> wgpu::Buffer {
	let descriptor = wgpu::util::BufferInitDescriptor {
		label:    Some("[zsw::panel] Vertex buffer"),
		contents: bytemuck::cast_slice(&PanelVertex::QUAD),
		usage:    wgpu::BufferUsages::VERTEX,
	};

	device.create_buffer_init(&descriptor)
}

/// Creates the indices
fn create_indices(device: &wgpu::Device) -> wgpu::Buffer {
	const INDICES: [u32; 6] = [0, 1, 3, 0, 3, 2];
	let descriptor = wgpu::util::BufferInitDescriptor {
		label:    Some("[zsw::panel] Index buffer"),
		contents: bytemuck::cast_slice(&INDICES),
		usage:    wgpu::BufferUsages::INDEX,
	};

	device.create_buffer_init(&descriptor)
}


/// Creates the image bind group layout
fn create_image_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
	let descriptor = wgpu::BindGroupLayoutDescriptor {
		label:   Some("[zsw::panel] Image bind group layout"),
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
	};

	device.create_bind_group_layout(&descriptor)
}

/// Creates the uniforms bind group layout
fn create_uniforms_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
	let descriptor = wgpu::BindGroupLayoutDescriptor {
		label:   Some("[zsw::panel] Uniform bind group layout"),
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
	};

	device.create_bind_group_layout(&descriptor)
}

/// Creates the render pipeline
fn create_render_pipeline(
	device: &wgpu::Device,
	surface_texture_format: wgpu::TextureFormat,
	uniforms_bind_group_layout: &wgpu::BindGroupLayout,
	texture_bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
	// Load the shader
	let shader_descriptor = wgpu::ShaderModuleDescriptor {
		label:  Some("[zsw::panel] Shader"),
		source: wgpu::ShaderSource::Wgsl(include_str!("renderer/shader.wgsl").into()),
	};
	let shader = device.create_shader_module(&shader_descriptor);

	// Create the pipeline layout
	let render_pipeline_layout_descriptor = wgpu::PipelineLayoutDescriptor {
		label:                Some("[zsw::panel] Render pipeline layout"),
		bind_group_layouts:   &[uniforms_bind_group_layout, texture_bind_group_layout],
		push_constant_ranges: &[],
	};
	let render_pipeline_layout = device.create_pipeline_layout(&render_pipeline_layout_descriptor);

	let color_targets = [wgpu::ColorTargetState {
		format:     surface_texture_format,
		blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
		write_mask: wgpu::ColorWrites::ALL,
	}];
	let render_pipeline_descriptor = wgpu::RenderPipelineDescriptor {
		label:         Some("[zsw::panel] Render pipeline"),
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

	device.create_render_pipeline(&render_pipeline_descriptor)
}
