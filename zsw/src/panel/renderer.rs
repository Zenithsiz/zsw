//! Panels renderer

// Modules
mod uniform;
mod vertex;

// Exports
pub use self::{uniform::PanelUniforms, vertex::PanelVertex};

// Imports
use {
	self::uniform::PanelImageUniforms,
	super::{Panel, PanelImage},
	crate::panel::PanelGeometry,
	anyhow::Context,
	cgmath::Point2,
	std::path::{Path, PathBuf},
	wgpu::util::DeviceExt,
	winit::dpi::PhysicalSize,
	zsw_error::AppError,
	zsw_util::Tpp,
	zsw_wgpu::{FrameRender, WgpuRenderer, WgpuShared},
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
	#[expect(clippy::too_many_arguments)] // TODO: Refactor
	pub fn render(
		&mut self,
		frame: &mut FrameRender,
		wgpu_renderer: &WgpuRenderer,
		wgpu_shared: &WgpuShared,
		layouts: &PanelsRendererLayouts,
		cursor_pos: Point2<i32>,
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
					store: true,
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
					store: false,
				},
			},
		};
		let render_pass_descriptor = wgpu::RenderPassDescriptor {
			label:                    Some("[zsw::panel_renderer] Render pass"),
			color_attachments:        &[Some(render_pass_color_attachment)],
			depth_stencil_attachment: None,
		};
		let surface_size = frame.surface_size();
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
				// Calculate the position matrix for the panel
				let pos_matrix = geometry.pos_matrix(surface_size);

				let create_uniforms = |image: &PanelImage, progress| {
					let ratio = PanelGeometry::image_ratio(geometry.geometry.size, image.size());
					let (parallax_ratio, parallax_offset) = geometry.parallax_ratio_offset(
						ratio,
						cursor_pos,
						panel.state.parallax.ratio,
						panel.state.parallax.exp,
						panel.state.parallax.reverse,
					);
					let progress = match image.swap_dir() {
						true => 1.0 - progress,
						false => progress,
					};
					PanelImageUniforms::new(ratio, progress, parallax_ratio, parallax_offset)
				};

				let front_uniforms = create_uniforms(panel.images.front(), panel.state.front_progress_norm());
				let back_uniforms = create_uniforms(panel.images.back(), panel.state.back_progress_norm());

				/// Writes uniforms with `$extra` into `panel.uniforms`
				macro write_uniforms($extra:expr) {{
					let uniforms = PanelUniforms::new(
						pos_matrix,
						front_uniforms,
						back_uniforms,
						panel.state.front_alpha(),
						$extra,
					);
					wgpu_shared
						.queue
						.write_buffer(&geometry.uniforms, 0, uniforms.as_bytes())
				}}

				// Update the uniforms
				match self.cur_shader {
					PanelShader::None => write_uniforms!(uniform::NoneExtra {}),
					PanelShader::Fade => write_uniforms!(uniform::FadeExtra {}),
					PanelShader::FadeWhite { strength } => write_uniforms!(uniform::FadeWhiteExtra { strength }),
					PanelShader::FadeOut { strength } => write_uniforms!(uniform::FadeOutExtra { strength }),
					PanelShader::FadeIn { strength } => write_uniforms!(uniform::FadeInExtra { strength }),
				};

				// Then bind the geometry uniforms and draw
				render_pass.set_bind_group(0, &geometry.uniforms_bind_group, &[]);
				render_pass.draw_indexed(0..6, 0, 0..1);
			}
		}

		Ok(())
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
		PanelShader::Fade => tpp.define("SHADER", "fade"),
		PanelShader::FadeWhite { .. } => tpp.define("SHADER", "fade-white"),
		PanelShader::FadeOut { .. } => tpp.define("SHADER", "fade-out"),
		PanelShader::FadeIn { .. } => tpp.define("SHADER", "fade-in"),
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
		label:         Some("[zsw::panel_renderer] Render pipeline"),
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
