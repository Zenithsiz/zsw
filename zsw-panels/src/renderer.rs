//! Panels renderer

// Modules
mod uniform;
mod vertex;

// Exports
pub use self::{uniform::PanelUniforms, vertex::PanelVertex};

// Imports
use {
	crate::{PanelsResource, PanelsShader},
	cgmath::Point2,
	wgpu::util::DeviceExt,
	winit::dpi::PhysicalSize,
	zsw_img::{ImageReceiver, RawImageProvider},
	zsw_wgpu::{Wgpu, WgpuResizeReceiver, WgpuSurfaceResource},
};

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
	/// Pipelines
	pipelines: PanelsPipelines,

	/// Vertex buffer
	vertices: wgpu::Buffer,

	/// Index buffer
	indices: wgpu::Buffer,

	/// Uniforms bind group layout
	uniforms_bind_group_layout: wgpu::BindGroupLayout,

	/// Image bind group layout
	image_bind_group_layout: wgpu::BindGroupLayout,

	/// Msaa frame-buffer
	msaa_framebuffer: wgpu::TextureView,

	/// Wgpu
	wgpu: Wgpu,

	/// Wgpu resizer
	wgpu_resize_receiver: WgpuResizeReceiver,
}

impl PanelsRenderer {
	/// Creates a new renderer for the panels
	#[must_use]
	pub fn new(
		wgpu: Wgpu,
		surface_resource: &mut WgpuSurfaceResource,
		wgpu_resize_receiver: WgpuResizeReceiver,
	) -> Self {
		// Create the index buffer
		let indices = self::create_indices(&wgpu);

		// Create the vertex buffer
		let vertices = self::create_vertices(&wgpu);

		// Create the bind group layouts
		let uniforms_bind_group_layout = self::create_uniforms_bind_group_layout(&wgpu);
		let image_bind_group_layout = self::create_image_bind_group_layout(&wgpu);

		// Create the render pipelines
		let fade_render_pipeline = self::create_render_pipeline(
			&wgpu,
			&uniforms_bind_group_layout,
			&image_bind_group_layout,
			include_str!("renderer/fade.wgsl"),
		);
		let fade_white_render_pipeline = self::create_render_pipeline(
			&wgpu,
			&uniforms_bind_group_layout,
			&image_bind_group_layout,
			include_str!("renderer/fade_white.wgsl"),
		);

		// Create the framebuffer
		let msaa_framebuffer = self::create_msaa_framebuffer(&wgpu, wgpu.surface_size(surface_resource));

		Self {
			pipelines: PanelsPipelines {
				fade:       fade_render_pipeline,
				fade_white: fade_white_render_pipeline,
			},
			vertices,
			indices,
			uniforms_bind_group_layout,
			image_bind_group_layout,
			msaa_framebuffer,
			wgpu,
			wgpu_resize_receiver,
		}
	}

	/// Returns the uniforms' bind group layout
	pub fn uniforms_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
		&self.uniforms_bind_group_layout
	}

	/// Returns the image bind group layout
	pub fn image_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
		&self.image_bind_group_layout
	}

	/// Updates all panels
	pub fn update_all<P: RawImageProvider>(
		&self,
		resource: &mut PanelsResource,
		wgpu: &Wgpu,
		image_receiver: &ImageReceiver<P>,
		max_image_size: Option<u32>,
	) -> Result<(), anyhow::Error> {
		for panel in &mut resource.panels {
			panel.update(self, wgpu, image_receiver, max_image_size);
		}

		Ok(())
	}

	/// Renders all panels
	#[allow(clippy::too_many_arguments)] // TODO: Refactor
	pub fn render(
		&mut self,
		resource: &PanelsResource,
		cursor_pos: Point2<i32>,
		queue: &wgpu::Queue,
		encoder: &mut wgpu::CommandEncoder,
		surface_view: &wgpu::TextureView,
		surface_size: PhysicalSize<u32>,
	) -> Result<(), anyhow::Error> {
		// Resize out msaa framebuffer if needed
		let last_resize = std::iter::from_fn(|| self.wgpu_resize_receiver.on_resize()).last();
		if let Some(size) = last_resize {
			tracing::debug!("Resizing msaa framebuffer to {}x{}", size.width, size.height);
			self.msaa_framebuffer = self::create_msaa_framebuffer(&self.wgpu, size);
		}

		// Create the render pass for all panels
		let render_pass_color_attachment = match MSAA_SAMPLES {
			1 => wgpu::RenderPassColorAttachment {
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
			},
			_ => wgpu::RenderPassColorAttachment {
				view:           &self.msaa_framebuffer,
				resolve_target: Some(surface_view),
				ops:            wgpu::Operations {
					load:  wgpu::LoadOp::Clear(wgpu::Color {
						r: 0.0,
						g: 0.0,
						b: 0.0,
						a: 1.0,
					}),
					store: false,
				},
			},
		};
		let render_pass_descriptor = wgpu::RenderPassDescriptor {
			label:                    Some("[zsw::panel] Render pass"),
			color_attachments:        &[Some(render_pass_color_attachment)],
			depth_stencil_attachment: None,
		};
		let mut render_pass = encoder.begin_render_pass(&render_pass_descriptor);

		// Set our shared pipeline, indices, vertices and uniform bind group
		let pipeline = match resource.shader {
			PanelsShader::Fade => &self.pipelines.fade,
			PanelsShader::FadeWhite { .. } => &self.pipelines.fade_white,
		};
		render_pass.set_pipeline(pipeline);
		render_pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
		render_pass.set_vertex_buffer(0, self.vertices.slice(..));

		// And draw each panel
		for panel in &resource.panels {
			// Calculate the position matrix for the panel
			let pos_matrix = panel.pos_matrix(surface_size);

			// Then go through all image descriptors to render
			for descriptor in panel.image_descriptors() {
				// Update the uniforms
				let uvs_matrix = descriptor.uvs_matrix(cursor_pos);
				let extra = match resource.shader {
					PanelsShader::Fade => [0.0; 3],
					PanelsShader::FadeWhite { strength } => [strength, 0.0, 0.0],
				};
				let uniforms = PanelUniforms::new(pos_matrix, uvs_matrix, descriptor.alpha(), extra);
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

/// Pipelines for [`PanelsRenderer`]
#[derive(Debug)]
pub struct PanelsPipelines {
	/// Fade Render pipeline
	fade: wgpu::RenderPipeline,

	/// Fade-white Render pipeline
	fade_white: wgpu::RenderPipeline,
}

/// Creates the vertices
fn create_vertices(wgpu: &Wgpu) -> wgpu::Buffer {
	let descriptor = wgpu::util::BufferInitDescriptor {
		label:    Some("[zsw::panel] Vertex buffer"),
		contents: bytemuck::cast_slice(&PanelVertex::QUAD),
		usage:    wgpu::BufferUsages::VERTEX,
	};

	wgpu.device().create_buffer_init(&descriptor)
}

/// Creates the indices
fn create_indices(wgpu: &Wgpu) -> wgpu::Buffer {
	const INDICES: [u32; 6] = [0, 1, 3, 0, 3, 2];
	let descriptor = wgpu::util::BufferInitDescriptor {
		label:    Some("[zsw::panel] Index buffer"),
		contents: bytemuck::cast_slice(&INDICES),
		usage:    wgpu::BufferUsages::INDEX,
	};

	wgpu.device().create_buffer_init(&descriptor)
}


/// Creates the image bind group layout
fn create_image_bind_group_layout(wgpu: &Wgpu) -> wgpu::BindGroupLayout {
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

	wgpu.device().create_bind_group_layout(&descriptor)
}

/// Creates the uniforms bind group layout
fn create_uniforms_bind_group_layout(wgpu: &Wgpu) -> wgpu::BindGroupLayout {
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

	wgpu.device().create_bind_group_layout(&descriptor)
}

/// Creates the render pipeline
fn create_render_pipeline(
	wgpu: &Wgpu,
	uniforms_bind_group_layout: &wgpu::BindGroupLayout,
	texture_bind_group_layout: &wgpu::BindGroupLayout,
	shader: &str,
) -> wgpu::RenderPipeline {
	// Load the shader
	let shader_descriptor = wgpu::ShaderModuleDescriptor {
		label:  Some("[zsw::panel] Shader"),
		source: wgpu::ShaderSource::Wgsl(shader.into()),
	};
	let shader = wgpu.device().create_shader_module(shader_descriptor);

	// Create the pipeline layout
	let render_pipeline_layout_descriptor = wgpu::PipelineLayoutDescriptor {
		label:                Some("[zsw::panel] Render pipeline layout"),
		bind_group_layouts:   &[uniforms_bind_group_layout, texture_bind_group_layout],
		push_constant_ranges: &[],
	};
	let render_pipeline_layout = wgpu.device().create_pipeline_layout(&render_pipeline_layout_descriptor);

	let color_targets = [Some(wgpu::ColorTargetState {
		format:     wgpu.surface_texture_format(),
		blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
		write_mask: wgpu::ColorWrites::ALL,
	})];
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
			count: MSAA_SAMPLES,
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

	wgpu.device().create_render_pipeline(&render_pipeline_descriptor)
}

/// Creates the msaa framebuffer
fn create_msaa_framebuffer(wgpu: &Wgpu, size: PhysicalSize<u32>) -> wgpu::TextureView {
	let msaa_texture_extent = wgpu::Extent3d {
		width:                 size.width,
		height:                size.height,
		depth_or_array_layers: 1,
	};

	let msaa_frame_descriptor = &wgpu::TextureDescriptor {
		size:            msaa_texture_extent,
		mip_level_count: 1,
		sample_count:    MSAA_SAMPLES,
		dimension:       wgpu::TextureDimension::D2,
		format:          wgpu.surface_texture_format(),
		usage:           wgpu::TextureUsages::RENDER_ATTACHMENT,
		label:           None,
	};

	wgpu.device()
		.create_texture(msaa_frame_descriptor)
		.create_view(&wgpu::TextureViewDescriptor::default())
}

/// MSAA samples
const MSAA_SAMPLES: u32 = 4;
