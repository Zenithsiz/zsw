//! Panels renderer

pub mod uniform;
mod vertex;

pub use self::vertex::PanelVertex;

use {
	super::{
		Panel,
		PanelState,
		Panels,
		state::{fade::PanelFadeShared, none::PanelNoneShared, slide::PanelSlideShared},
	},
	app_error::Context,
	euclid::default::Vector2D,
	std::{
		borrow::Cow,
		collections::{HashMap, hash_map},
		sync::Arc,
	},
	wgpu::util::DeviceExt,
	zsw_util::{AppError, Rect},
	zsw_wgpu::{FrameRender, Wgpu, WgpuRenderer},
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
	/// Msaa frame-buffer
	msaa_framebuffer: wgpu::TextureView,

	/// Massa samples
	// TODO: If we change this, we need to re-create the render pipelines too
	msaa_samples: u32,

	/// Render pipeline for each shader
	// TODO: Prune ones that aren't used?
	render_pipelines: HashMap<RenderPipelineId, wgpu::RenderPipeline>,

	/// Vertex buffer
	vertices: wgpu::Buffer,

	/// Index buffer
	indices: wgpu::Buffer,

	none_shared:  PanelNoneShared,
	fade_shared:  PanelFadeShared,
	slide_shared: PanelSlideShared,
}

impl PanelsRenderer {
	/// Creates a new renderer for the panels
	pub fn new(wgpu: &Wgpu, wgpu_renderer: &WgpuRenderer, msaa_samples: u32) -> Result<Self, AppError> {
		// Create the framebuffer
		let msaa_framebuffer =
			self::create_msaa_framebuffer(wgpu, wgpu_renderer, wgpu_renderer.surface_size(), msaa_samples);

		// Create the index / vertex buffer
		let indices = self::create_indices(wgpu);
		let vertices = self::create_vertices(wgpu);

		Ok(Self {
			msaa_framebuffer,
			msaa_samples,
			render_pipelines: HashMap::new(),
			vertices,
			indices,
			none_shared: PanelNoneShared::new(),
			fade_shared: PanelFadeShared::new(),
			slide_shared: PanelSlideShared::new(),
		})
	}

	/// Resizes the buffer
	pub fn resize(&mut self, wgpu: &Wgpu, wgpu_renderer: &WgpuRenderer, size: Vector2D<u32>) {
		tracing::debug!("Resizing msaa framebuffer to {}x{}", size.x, size.y);
		self.msaa_framebuffer = self::create_msaa_framebuffer(wgpu, wgpu_renderer, size, self.msaa_samples);
	}

	/// Renders a panel
	pub fn render(
		&mut self,
		wgpu: &Arc<Wgpu>,
		wgpu_renderer: &WgpuRenderer,
		surface_geometry: Rect<i32, u32>,
		frame: &mut FrameRender,
		panels: &mut Panels,
	) -> Result<(), AppError> {
		// Create the render pass for all panels
		let render_pass_color_attachment = match self.msaa_samples {
			1 => wgpu::RenderPassColorAttachment {
				view:           &frame.surface_view,
				depth_slice:    None,
				resolve_target: None,
				ops:            wgpu::Operations {
					load:  wgpu::LoadOp::Clear(wgpu::Color {
						r: 0.0,
						g: 0.0,
						b: 0.0,
						a: 0.0,
					}),
					store: wgpu::StoreOp::Store,
				},
			},
			_ => wgpu::RenderPassColorAttachment {
				view:           &self.msaa_framebuffer,
				depth_slice:    None,
				resolve_target: Some(&frame.surface_view),
				ops:            wgpu::Operations {
					load:  wgpu::LoadOp::Clear(wgpu::Color {
						r: 0.0,
						g: 0.0,
						b: 0.0,
						a: 0.0,
					}),
					store: wgpu::StoreOp::Discard,
				},
			},
		};
		let render_pass_descriptor = wgpu::RenderPassDescriptor {
			label:                    Some("zsw-panel-render-pass"),
			color_attachments:        &[Some(render_pass_color_attachment)],
			depth_stencil_attachment: None,
			timestamp_writes:         None,
			occlusion_query_set:      None,
			multiview_mask:           None,
		};
		let mut render_pass = frame.encoder.begin_render_pass(&render_pass_descriptor);

		// Set our shared indices and vertices
		render_pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
		render_pass.set_vertex_buffer(0, self.vertices.slice(..));

		// Then render all panels simultaneously
		for panel in panels.get_all() {
			self.render_panel(wgpu, wgpu_renderer, surface_geometry, &mut render_pass, panel)?;
		}

		Ok(())
	}

	/// Renders a panel
	fn render_panel(
		&mut self,
		wgpu: &Arc<Wgpu>,
		wgpu_renderer: &WgpuRenderer,
		surface_geometry: Rect<i32, u32>,
		render_pass: &mut wgpu::RenderPass<'_>,
		panel: &mut Panel,
	) -> Result<(), app_error::AppError> {
		// Update the panel before drawing it
		match &mut panel.state {
			PanelState::None(_) => (),
			PanelState::Fade(state) => state.update(wgpu),
			PanelState::Slide(state) => state.update(wgpu),
		}

		// If the panel images are empty, there's no sense in rendering it either
		#[expect(clippy::match_same_arms, reason = "We'll be changing them soon")]
		let are_images_empty = match &panel.state {
			PanelState::None(_) => false,
			PanelState::Fade(state) => state.images().is_empty(),
			PanelState::Slide(_) => false,
		};
		if are_images_empty {
			return Ok(());
		}

		let render_pipeline_id = match &panel.state {
			PanelState::None(_) => RenderPipelineId::None,
			PanelState::Fade(state) => RenderPipelineId::Fade(match state.shader() {
				PanelFadeShader::Basic => RenderPipelineFadeId::Basic,
				PanelFadeShader::Out { .. } => RenderPipelineFadeId::Out,
			}),
			PanelState::Slide(state) => RenderPipelineId::Slide(match state.shader() {
				PanelSlideShader::Basic => RenderPipelineSlideId::Basic,
			}),
		};

		let render_pipeline = match self.render_pipelines.entry(render_pipeline_id) {
			hash_map::Entry::Occupied(entry) => entry.into_mut(),
			hash_map::Entry::Vacant(entry) => {
				let bind_group_layouts = match panel.state {
					PanelState::None(_) =>
						&[Some(self.none_shared.geometry_uniforms_bind_group_layout(wgpu))] as &[_],
					PanelState::Fade(_) => &[
						Some(self.fade_shared.images.geometry_uniforms_bind_group_layout(wgpu)),
						Some(self.fade_shared.images.image_bind_group_layout(wgpu)),
					],
					PanelState::Slide(_) => &[
						Some(self.slide_shared.geometry_uniforms_bind_group_layout(wgpu)),
						Some(self.slide_shared.image_bind_group_layout(wgpu)),
					],
				};

				let render_pipeline = self::create_render_pipeline(
					wgpu,
					wgpu_renderer,
					render_pipeline_id,
					bind_group_layouts,
					panel.state.shader(),
					self.msaa_samples,
				)
				.context("Unable to create render pipeline")?;

				entry.insert(render_pipeline)
			},
		};

		// Bind the pipeline for the specific shader
		render_pass.set_pipeline(render_pipeline);

		// Then render the panel
		self.render_panel_geometries(wgpu, surface_geometry, render_pass, &mut panel.state);

		Ok(())
	}

	/// Renders a panel's geometries
	pub fn render_panel_geometries(
		&self,
		wgpu: &Arc<Wgpu>,
		surface_geometry: Rect<i32, u32>,
		render_pass: &mut wgpu::RenderPass<'_>,
		state: &mut PanelState,
	) {
		match state {
			PanelState::None(state) => state.render(&self.none_shared, wgpu, surface_geometry, render_pass),
			PanelState::Fade(state) => state.render(&self.fade_shared, wgpu, surface_geometry, render_pass),
			PanelState::Slide(state) => state.render(&self.slide_shared, wgpu, surface_geometry, render_pass),
		}
	}
}

/// Creates the vertices
fn create_vertices(wgpu: &Wgpu) -> wgpu::Buffer {
	let descriptor = wgpu::util::BufferInitDescriptor {
		label:    Some("zsw-panel-vertex-buffer"),
		contents: bytemuck::cast_slice(&PanelVertex::QUAD),
		usage:    wgpu::BufferUsages::VERTEX,
	};

	wgpu.device.create_buffer_init(&descriptor)
}

/// Creates the indices
fn create_indices(wgpu: &Wgpu) -> wgpu::Buffer {
	const INDICES: [u32; 6] = [0, 1, 3, 0, 3, 2];
	let descriptor = wgpu::util::BufferInitDescriptor {
		label:    Some("zsw-panel-index-buffer"),
		contents: bytemuck::cast_slice(&INDICES),
		usage:    wgpu::BufferUsages::INDEX,
	};

	wgpu.device.create_buffer_init(&descriptor)
}

/// Render pipeline id
#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum RenderPipelineId {
	/// None shader
	None,

	/// Fade shader
	Fade(RenderPipelineFadeId),

	/// Slide
	Slide(RenderPipelineSlideId),
}

impl RenderPipelineId {
	/// Returns this pipeline's name
	pub fn name(self) -> &'static str {
		match self {
			Self::None => "none",
			Self::Fade(id) => id.name(),
			Self::Slide(id) => id.name(),
		}
	}
}

/// Render pipeline fade id
#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum RenderPipelineFadeId {
	Basic,
	Out,
}

impl RenderPipelineFadeId {
	/// Returns this pipeline's name
	pub fn name(self) -> &'static str {
		match self {
			Self::Basic => "fade-basic",
			Self::Out => "fade-out",
		}
	}
}

/// Render pipeline slide id
#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum RenderPipelineSlideId {
	Basic,
}

impl RenderPipelineSlideId {
	/// Returns this pipeline's name
	pub fn name(self) -> &'static str {
		match self {
			Self::Basic => "slide-basic",
		}
	}
}

/// Creates the render pipeline
fn create_render_pipeline(
	wgpu: &Wgpu,
	wgpu_renderer: &WgpuRenderer,
	id: RenderPipelineId,
	bind_group_layouts: &[Option<&wgpu::BindGroupLayout>],
	shader: PanelShader,
	msaa_samples: u32,
) -> Result<wgpu::RenderPipeline, AppError> {
	let render_pipeline_name = id.name();
	let shader_name = shader.name();
	tracing::debug!("Creating render pipeline {render_pipeline_name:?} for shader {shader_name:?}");

	// Parse the shader from the build script
	let shader_module =
		serde_json::from_str::<naga::Module>(shader.module_json()).context("Serialized shader module was invalid")?;

	// Load the shader
	let shader_descriptor = wgpu::ShaderModuleDescriptor {
		label:  Some(&format!("zsw-panel-shader[name={shader_name:?}]")),
		source: wgpu::ShaderSource::Naga(Cow::Owned(shader_module)),
	};
	let shader = wgpu.device.create_shader_module(shader_descriptor);

	// Create the pipeline layout
	let render_pipeline_layout_descriptor = wgpu::PipelineLayoutDescriptor {
		label: Some(&format!(
			"zsw-panel-render-pipeline[name={render_pipeline_name:?}]-layout"
		)),
		bind_group_layouts,
		immediate_size: 0,
	};
	let render_pipeline_layout = wgpu.device.create_pipeline_layout(&render_pipeline_layout_descriptor);

	let color_targets = [Some(wgpu::ColorTargetState {
		format:     wgpu_renderer.surface_config.format,
		blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
		write_mask: wgpu::ColorWrites::ALL,
	})];
	let render_pipeline_descriptor = wgpu::RenderPipelineDescriptor {
		label:  Some(&format!("zsw-panel-render-pipeline[name={render_pipeline_name:?}]")),
		layout: Some(&render_pipeline_layout),

		vertex:         wgpu::VertexState {
			module:              &shader,
			entry_point:         Some("vs_main"),
			buffers:             &[Some(PanelVertex::buffer_layout())],
			compilation_options: wgpu::PipelineCompilationOptions::default(),
		},
		primitive:      wgpu::PrimitiveState {
			topology:           wgpu::PrimitiveTopology::TriangleList,
			strip_index_format: None,
			front_face:         wgpu::FrontFace::Ccw,
			cull_mode:          None,
			unclipped_depth:    false,
			polygon_mode:       wgpu::PolygonMode::Fill,
			conservative:       false,
		},
		depth_stencil:  None,
		multisample:    wgpu::MultisampleState {
			count: msaa_samples,
			mask: u64::MAX,
			alpha_to_coverage_enabled: false,
		},
		fragment:       Some(wgpu::FragmentState {
			module:              &shader,
			entry_point:         Some("fs_main"),
			targets:             &color_targets,
			compilation_options: wgpu::PipelineCompilationOptions::default(),
		}),
		multiview_mask: None,
		cache:          None,
	};

	Ok(wgpu.device.create_render_pipeline(&render_pipeline_descriptor))
}

/// Creates the msaa framebuffer
fn create_msaa_framebuffer(
	wgpu: &Wgpu,
	wgpu_renderer: &WgpuRenderer,
	size: Vector2D<u32>,
	msaa_samples: u32,
) -> wgpu::TextureView {
	let msaa_texture_extent = wgpu::Extent3d {
		width:                 size.x,
		height:                size.y,
		depth_or_array_layers: 1,
	};

	let msaa_frame_descriptor = wgpu::TextureDescriptor {
		label:           Some("zsw-panel-framebuffer-msaa"),
		size:            msaa_texture_extent,
		mip_level_count: 1,
		sample_count:    msaa_samples,
		dimension:       wgpu::TextureDimension::D2,
		format:          wgpu_renderer.surface_config.format,
		usage:           wgpu::TextureUsages::RENDER_ATTACHMENT,
		view_formats:    &wgpu_renderer.surface_config.view_formats,
	};

	wgpu.device
		.create_texture(&msaa_frame_descriptor)
		.create_view(&wgpu::TextureViewDescriptor {
			label: Some("zsw-panel-framebuffer-msaa-view"),
			..Default::default()
		})
}

/// Shader
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum PanelShader {
	/// None shader
	None { background_color: [f32; 4] },

	/// Fade shader
	Fade(PanelFadeShader),

	/// Slide shader
	Slide(PanelSlideShader),
}

impl PanelShader {
	/// Returns this shader's name
	pub fn name(self) -> &'static str {
		match self {
			Self::None { .. } => "None",
			Self::Fade(fade) => fade.name(),
			Self::Slide(slide) => slide.name(),
		}
	}

	/// Returns this shader's module as json
	pub fn module_json(self) -> &'static str {
		match self {
			Self::None { .. } => include_str!(concat!(env!("OUT_DIR"), "/shaders/panels/none.json")),
			Self::Fade(fade) => fade.module_json(),
			Self::Slide(slide) => slide.module_json(),
		}
	}
}

/// Panel fade shader
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum PanelFadeShader {
	Basic,
	Out { strength: f32 },
}

impl PanelFadeShader {
	/// Returns this shader's name
	pub fn name(self) -> &'static str {
		match self {
			Self::Basic => "Fade",
			Self::Out { .. } => "Fade out",
		}
	}

	/// Returns this shader's module as json
	pub fn module_json(self) -> &'static str {
		match self {
			Self::Basic => include_str!(concat!(env!("OUT_DIR"), "/shaders/panels/fade.json")),
			Self::Out { .. } => include_str!(concat!(env!("OUT_DIR"), "/shaders/panels/fade-out.json")),
		}
	}
}

/// Panel slide shader
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum PanelSlideShader {
	Basic,
}

impl PanelSlideShader {
	/// Returns this shader's name
	pub fn name(self) -> &'static str {
		match self {
			Self::Basic => "Slide",
		}
	}

	/// Returns this shader's module as json
	pub fn module_json(self) -> &'static str {
		match self {
			Self::Basic => include_str!(concat!(env!("OUT_DIR"), "/shaders/panels/slide.json")),
		}
	}
}
