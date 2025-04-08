//! Panels renderer

// Modules
mod uniform;
mod vertex;

// Exports
pub use self::vertex::PanelVertex;

// Imports
use {
	self::uniform::PanelImageUniforms,
	super::{Panel, PanelImage},
	crate::panel::PanelGeometry,
	cgmath::Vector2,
	std::path::{Path, PathBuf},
	wgpu::util::DeviceExt,
	winit::dpi::PhysicalSize,
	zsw_util::Tpp,
	zsw_wgpu::{FrameRender, WgpuRenderer, WgpuShared},
	zutil_app_error::{AppError, Context},
};

/// Panels renderer layouts
#[derive(Debug)]
pub struct PanelsRendererLayouts {
	/// Uniforms bind group layout
	pub uniforms_bind_group_layout: wgpu::BindGroupLayout,

	/// Image bind group layout
	pub image_bind_group_layout: wgpu::BindGroupLayout,
}

/// Panels renderer shader
#[derive(Debug)]
pub struct PanelsRendererShader {
	/// Current shader
	pub shader: PanelShader,

	/// Shader path
	pub shader_path: PathBuf,
}

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

	/// Msaa frame-buffer
	msaa_framebuffer: wgpu::TextureView,

	/// Current shader
	cur_shader: PanelShader,
}

impl PanelsRenderer {
	/// Creates a new renderer for the panels
	pub fn new(
		wgpu_renderer: &WgpuRenderer,
		wgpu_shared: &WgpuShared,
		shader_path: PathBuf,
	) -> Result<(Self, PanelsRendererLayouts, PanelsRendererShader), AppError> {
		// Create the index / vertex buffer
		let indices = self::create_indices(wgpu_shared);
		let vertices = self::create_vertices(wgpu_shared);

		// Create the framebuffer
		let msaa_framebuffer = self::create_msaa_framebuffer(wgpu_renderer, wgpu_shared, wgpu_renderer.surface_size());

		// Create the group layouts
		let uniforms_bind_group_layout = self::create_uniforms_bind_group_layout(wgpu_shared);
		let image_bind_group_layout = self::create_image_bind_group_layout(wgpu_shared);

		// By default use the empty shader
		let shader = PanelShader::None;

		Ok((
			Self {
				render_pipeline: self::create_render_pipeline(
					wgpu_renderer,
					wgpu_shared,
					&uniforms_bind_group_layout,
					&image_bind_group_layout,
					shader,
					&shader_path,
				)
				.context("Unable to create render pipeline")?,
				vertices,
				indices,
				msaa_framebuffer,
				cur_shader: shader,
			},
			PanelsRendererLayouts {
				uniforms_bind_group_layout,
				image_bind_group_layout,
			},
			PanelsRendererShader { shader, shader_path },
		))
	}

	/// Resizes the buffer
	pub fn resize(&mut self, wgpu_renderer: &WgpuRenderer, wgpu_shared: &WgpuShared, size: PhysicalSize<u32>) {
		tracing::debug!("Resizing msaa framebuffer to {}x{}", size.width, size.height);
		self.msaa_framebuffer = self::create_msaa_framebuffer(wgpu_renderer, wgpu_shared, size);
	}

	/// Updates the shader.
	///
	/// Returns if a pipeline reload is necessary
	fn update_shader(&mut self, shader: PanelShader) -> bool {
		let needs_reload = match (self.cur_shader, shader) {
			// If we're the same kind, no need to reload the pipeline
			(PanelShader::Fade, PanelShader::Fade) |
			(PanelShader::FadeWhite { .. }, PanelShader::FadeWhite { .. }) |
			(PanelShader::FadeOut { .. }, PanelShader::FadeOut { .. }) |
			(PanelShader::FadeIn { .. }, PanelShader::FadeIn { .. }) => false,

			// Else reload it
			_ => true,
		};

		self.cur_shader = shader;

		needs_reload
	}

	/// Renders a panel
	pub fn render(
		&mut self,
		frame: &mut FrameRender,
		wgpu_renderer: &WgpuRenderer,
		wgpu_shared: &WgpuShared,
		layouts: &PanelsRendererLayouts,
		panels: impl IntoIterator<Item = &'_ Panel>,
		shader: &PanelsRendererShader,
	) -> Result<(), AppError> {
		// Update the shader, if requested
		if self.update_shader(shader.shader) {
			self.render_pipeline = self::create_render_pipeline(
				wgpu_renderer,
				wgpu_shared,
				&layouts.uniforms_bind_group_layout,
				&layouts.image_bind_group_layout,
				shader.shader,
				&shader.shader_path,
			)
			.context("Unable to create render pipeline")?;
		}

		// Create the render pass for all panels
		let render_pass_color_attachment = match MSAA_SAMPLES {
			1 => wgpu::RenderPassColorAttachment {
				view:           &frame.surface_view,
				resolve_target: None,
				ops:            wgpu::Operations {
					load:  wgpu::LoadOp::Clear(wgpu::Color {
						r: 0.0,
						g: 0.0,
						b: 0.0,
						a: 1.0,
					}),
					store: wgpu::StoreOp::Store,
				},
			},
			_ => wgpu::RenderPassColorAttachment {
				view:           &self.msaa_framebuffer,
				resolve_target: Some(&frame.surface_view),
				ops:            wgpu::Operations {
					load:  wgpu::LoadOp::Clear(wgpu::Color {
						r: 0.0,
						g: 0.0,
						b: 0.0,
						a: 1.0,
					}),
					store: wgpu::StoreOp::Discard,
				},
			},
		};
		let render_pass_descriptor = wgpu::RenderPassDescriptor {
			label:                    Some("[zsw::panel_renderer] Render pass"),
			color_attachments:        &[Some(render_pass_color_attachment)],
			depth_stencil_attachment: None,
			timestamp_writes:         None,
			occlusion_query_set:      None,
		};
		let mut render_pass = frame.encoder.begin_render_pass(&render_pass_descriptor);

		// Set our shared pipeline, indices, vertices and uniform bind group
		render_pass.set_pipeline(&self.render_pipeline);
		render_pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
		render_pass.set_vertex_buffer(0, self.vertices.slice(..));

		// And draw each panel
		for panel in panels {
			// Bind the panel-shared image bind group
			render_pass.set_bind_group(1, panel.images.image_bind_group(), &[]);

			for geometry in &panel.geometries {
				// Write the uniforms
				self.write_uniforms(wgpu_shared, frame.surface_size, panel, geometry);

				// Then bind the geometry uniforms and draw
				render_pass.set_bind_group(0, &geometry.uniforms_bind_group, &[]);
				render_pass.draw_indexed(0..6, 0, 0..1);
			}
		}

		Ok(())
	}

	/// Writes the uniforms
	pub fn write_uniforms(
		&self,
		wgpu_shared: &WgpuShared,
		surface_size: PhysicalSize<u32>,
		panel: &Panel,
		geometry: &PanelGeometry,
	) {
		// Calculate the position matrix for the panel
		let pos_matrix = geometry.pos_matrix(surface_size);
		let pos_matrix = uniform::Matrix4x4(pos_matrix.into());

		let image_uniforms = |image: &PanelImage| {
			let (size, swap_dir) = match *image {
				PanelImage::Empty => (Vector2::new(0, 0), false),
				PanelImage::Loaded { size, swap_dir, .. } => (size, swap_dir),
			};

			let ratio = PanelGeometry::image_ratio(geometry.geometry.size, size);
			PanelImageUniforms::new(ratio, swap_dir)
		};

		let prev = image_uniforms(panel.images.prev());
		let cur = image_uniforms(panel.images.cur());
		let next = image_uniforms(panel.images.next());

		// Writes uniforms `uniforms`
		let write_uniforms = |uniforms_bytes| wgpu_shared.queue.write_buffer(&geometry.uniforms, 0, uniforms_bytes);
		macro write_uniforms($uniforms:expr) {
			write_uniforms(bytemuck::bytes_of(&$uniforms))
		}

		let fade_point = panel.state.fade_point_norm();
		let progress = panel.state.progress_norm();
		match self.cur_shader {
			PanelShader::None => write_uniforms!(uniform::None { pos_matrix }),
			PanelShader::Fade => write_uniforms!(uniform::Fade {
				pos_matrix,
				prev,
				cur,
				next,
				fade_point,
				progress,
				_unused: [0; 2],
			}),
			PanelShader::FadeWhite { strength } => write_uniforms!(uniform::FadeWhite {
				pos_matrix,
				prev,
				cur,
				next,
				fade_point,
				progress,
				strength,
				_unused: 0,
			}),
			PanelShader::FadeOut { strength } => write_uniforms!(uniform::FadeOut {
				pos_matrix,
				prev,
				cur,
				next,
				fade_point,
				progress,
				strength,
				_unused: 0,
			}),
			PanelShader::FadeIn { strength } => write_uniforms!(uniform::FadeIn {
				pos_matrix,
				prev,
				cur,
				next,
				fade_point,
				progress,
				strength,
				_unused: 0,
			}),
		};
	}
}

/// Creates the vertices
fn create_vertices(wgpu_shared: &WgpuShared) -> wgpu::Buffer {
	let descriptor = wgpu::util::BufferInitDescriptor {
		label:    Some("[zsw::panel_renderer] Vertex buffer"),
		contents: bytemuck::cast_slice(&PanelVertex::QUAD),
		usage:    wgpu::BufferUsages::VERTEX,
	};

	wgpu_shared.device.create_buffer_init(&descriptor)
}

/// Creates the indices
fn create_indices(wgpu_shared: &WgpuShared) -> wgpu::Buffer {
	const INDICES: [u32; 6] = [0, 1, 3, 0, 3, 2];
	let descriptor = wgpu::util::BufferInitDescriptor {
		label:    Some("[zsw::panel_renderer] Index buffer"),
		contents: bytemuck::cast_slice(&INDICES),
		usage:    wgpu::BufferUsages::INDEX,
	};

	wgpu_shared.device.create_buffer_init(&descriptor)
}

/// Creates the render pipeline
fn create_render_pipeline(
	wgpu_renderer: &WgpuRenderer,
	wgpu_shared: &WgpuShared,
	uniforms_bind_group_layout: &wgpu::BindGroupLayout,
	image_bind_group_layout: &wgpu::BindGroupLayout,
	shader: PanelShader,
	shader_path: &Path,
) -> Result<wgpu::RenderPipeline, AppError> {
	tracing::debug!(?shader, ?shader_path, "Creating render pipeline for shader");

	// Parse the shader
	let mut tpp = Tpp::new();
	match shader {
		PanelShader::None => tpp.define("SHADER", "none"),
		PanelShader::Fade => {
			tpp.define("SHADER", "fade");
			tpp.define("SHADER_FADE_TYPE", "fade");
		},
		PanelShader::FadeWhite { .. } => {
			tpp.define("SHADER", "fade");
			tpp.define("SHADER_FADE_TYPE", "white");
		},
		PanelShader::FadeOut { .. } => {
			tpp.define("SHADER", "fade");
			tpp.define("SHADER_FADE_TYPE", "out");
		},
		PanelShader::FadeIn { .. } => {
			tpp.define("SHADER", "fade");
			tpp.define("SHADER_FADE_TYPE", "in");
		},
	};
	let shader_contents = tpp
		.process(shader_path)
		.with_context(|| format!("Unable to preprocess shader {shader_path:?}"))?;

	// Load the shader
	let shader_descriptor = wgpu::ShaderModuleDescriptor {
		label:  Some("[zsw::panel_renderer] Shader"),
		source: wgpu::ShaderSource::Wgsl(shader_contents.into()),
	};
	let shader = wgpu_shared.device.create_shader_module(shader_descriptor);

	// Create the pipeline layout
	let render_pipeline_layout_descriptor = wgpu::PipelineLayoutDescriptor {
		label:                Some("[zsw::panel_renderer] Render pipeline layout"),
		bind_group_layouts:   &[uniforms_bind_group_layout, image_bind_group_layout],
		push_constant_ranges: &[],
	};
	let render_pipeline_layout = wgpu_shared
		.device
		.create_pipeline_layout(&render_pipeline_layout_descriptor);

	let color_targets = [Some(wgpu::ColorTargetState {
		format:     wgpu_renderer.surface_config().format,
		blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
		write_mask: wgpu::ColorWrites::ALL,
	})];
	let render_pipeline_descriptor = wgpu::RenderPipelineDescriptor {
		label:  Some("[zsw::panel_renderer] Render pipeline"),
		layout: Some(&render_pipeline_layout),

		vertex:        wgpu::VertexState {
			module:              &shader,
			entry_point:         "vs_main",
			buffers:             &[PanelVertex::buffer_layout()],
			compilation_options: wgpu::PipelineCompilationOptions::default(),
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
			module:              &shader,
			entry_point:         "fs_main",
			targets:             &color_targets,
			compilation_options: wgpu::PipelineCompilationOptions::default(),
		}),
		multiview:     None,
		cache:         None,
	};

	Ok(wgpu_shared.device.create_render_pipeline(&render_pipeline_descriptor))
}

/// Creates the msaa framebuffer
fn create_msaa_framebuffer(
	wgpu_renderer: &WgpuRenderer,
	wgpu_shared: &WgpuShared,
	size: PhysicalSize<u32>,
) -> wgpu::TextureView {
	let msaa_texture_extent = wgpu::Extent3d {
		width:                 size.width,
		height:                size.height,
		depth_or_array_layers: 1,
	};

	let surface_config = wgpu_renderer.surface_config();
	let msaa_frame_descriptor = &wgpu::TextureDescriptor {
		size:            msaa_texture_extent,
		mip_level_count: 1,
		sample_count:    MSAA_SAMPLES,
		dimension:       wgpu::TextureDimension::D2,
		format:          surface_config.format,
		usage:           wgpu::TextureUsages::RENDER_ATTACHMENT,
		label:           Some("[zsw::panel_renderer] MSAA framebuffer"),
		view_formats:    &surface_config.view_formats,
	};

	wgpu_shared
		.device
		.create_texture(msaa_frame_descriptor)
		.create_view(&wgpu::TextureViewDescriptor::default())
}

/// MSAA samples
const MSAA_SAMPLES: u32 = 4;

/// Creates the uniforms bind group layout
fn create_uniforms_bind_group_layout(wgpu_shared: &WgpuShared) -> wgpu::BindGroupLayout {
	let descriptor = wgpu::BindGroupLayoutDescriptor {
		label:   Some("[zsw::panel_renderer] Uniform bind group layout"),
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

	wgpu_shared.device.create_bind_group_layout(&descriptor)
}

/// Creates the image bind group layout
fn create_image_bind_group_layout(wgpu_shared: &WgpuShared) -> wgpu::BindGroupLayout {
	let descriptor = wgpu::BindGroupLayoutDescriptor {
		label:   Some("[zsw::panel_renderer] Image bind group layout"),
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
				ty:         wgpu::BindingType::Texture {
					multisampled:   false,
					view_dimension: wgpu::TextureViewDimension::D2,
					sample_type:    wgpu::TextureSampleType::Float { filterable: true },
				},
				count:      None,
			},
			wgpu::BindGroupLayoutEntry {
				binding:    2,
				visibility: wgpu::ShaderStages::FRAGMENT,
				ty:         wgpu::BindingType::Texture {
					multisampled:   false,
					view_dimension: wgpu::TextureViewDimension::D2,
					sample_type:    wgpu::TextureSampleType::Float { filterable: true },
				},
				count:      None,
			},
			wgpu::BindGroupLayoutEntry {
				binding:    3,
				visibility: wgpu::ShaderStages::FRAGMENT,
				ty:         wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
				count:      None,
			},
		],
	};

	wgpu_shared.device.create_bind_group_layout(&descriptor)
}

/// Shader
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum PanelShader {
	None,
	Fade,
	FadeWhite { strength: f32 },
	FadeOut { strength: f32 },
	FadeIn { strength: f32 },
}
impl PanelShader {
	/// Returns this shader's name
	pub fn name(self) -> &'static str {
		match self {
			Self::None => "None",
			Self::Fade => "Fade",
			Self::FadeWhite { .. } => "Fade white",
			Self::FadeOut { .. } => "Fade out",
			Self::FadeIn { .. } => "Fade in",
		}
	}
}
