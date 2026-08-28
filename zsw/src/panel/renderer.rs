//! Panels renderer

pub mod uniform;
mod vertex;

pub use self::vertex::PanelVertex;

use {
	super::{
		Panel,
		PanelGeometry,
		PanelState,
		state::{
			PanelFadeState,
			PanelNoneState,
			PanelSlideState,
			fade::{PanelFadeImage, PanelFadeImageSlot, PanelFadeShared},
			none::PanelNoneShared,
			slide::PanelSlideShared,
		},
	},
	crate::shared::{Shared, SharedWindow},
	app_error::Context,
	core::{clone::Share, cmp},
	euclid::default::{Transform3D, Vector2D},
	std::{
		borrow::Cow,
		collections::{HashMap, hash_map},
		sync::{Arc, OnceLock, nonpoison::Mutex},
	},
	wgpu::util::DeviceExt,
	winit::dpi::PhysicalSize,
	zsw_util::{AppError, Rect},
	zsw_wgpu::{FrameRender, Wgpu, WgpuRenderer},
};

/// Panels renderer shared
#[derive(Debug)]
pub struct PanelsRendererShared {
	/// Render pipeline for each shader
	// TODO: Prune ones that aren't used?
	render_pipelines: Mutex<HashMap<RenderPipelineId, Arc<wgpu::RenderPipeline>>>,

	/// Vertex buffer
	vertices: wgpu::Buffer,

	/// Index buffer
	indices: wgpu::Buffer,

	/// None
	none: OnceLock<PanelNoneShared>,

	/// Fade
	fade: OnceLock<PanelFadeShared>,

	/// Slide
	slide: OnceLock<PanelSlideShared>,
}

impl PanelsRendererShared {
	/// Creates new layouts for the panels renderer
	pub fn new(wgpu: &Wgpu) -> Self {
		// Create the index / vertex buffer
		let indices = self::create_indices(wgpu);
		let vertices = self::create_vertices(wgpu);

		Self {
			render_pipelines: Mutex::new(HashMap::new()),
			vertices,
			indices,
			none: OnceLock::new(),
			fade: OnceLock::new(),
			slide: OnceLock::new(),
		}
	}

	/// Gets the shared none data
	pub fn none(&self, wgpu: &Wgpu) -> &PanelNoneShared {
		self.none.get_or_init(|| PanelNoneShared::new(wgpu))
	}

	/// Gets the shared fade data
	pub fn fade(&self, wgpu: &Wgpu) -> &PanelFadeShared {
		self.fade.get_or_init(|| PanelFadeShared::new(wgpu))
	}

	/// Gets the shared slide data
	pub fn slide(&self, wgpu: &Wgpu) -> &PanelSlideShared {
		self.slide.get_or_init(|| PanelSlideShared::new(wgpu))
	}
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
	/// Msaa frame-buffer
	msaa_framebuffer: wgpu::TextureView,

	/// Massa samples
	// TODO: If we change this, we need to re-create the render pipelines too
	msaa_samples: u32,
}

impl PanelsRenderer {
	/// Creates a new renderer for the panels
	pub fn new(wgpu_renderer: &WgpuRenderer, wgpu: &Wgpu, msaa_samples: u32) -> Result<Self, AppError> {
		// Create the framebuffer
		let msaa_framebuffer =
			self::create_msaa_framebuffer(wgpu_renderer, wgpu, wgpu_renderer.surface_size(), msaa_samples);

		Ok(Self {
			msaa_framebuffer,
			msaa_samples,
		})
	}

	/// Resizes the buffer
	pub fn resize(&mut self, wgpu_renderer: &WgpuRenderer, wgpu: &Wgpu, size: PhysicalSize<u32>) {
		tracing::debug!("Resizing msaa framebuffer to {}x{}", size.width, size.height);
		self.msaa_framebuffer = self::create_msaa_framebuffer(wgpu_renderer, wgpu, size, self.msaa_samples);
	}

	/// Renders a panel
	pub fn render(
		&self,
		shared: &Shared,
		shared_window: &mut SharedWindow,
		frame: &mut FrameRender,
		wgpu_renderer: &WgpuRenderer,
		panels_shared: &PanelsRendererShared,
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
		render_pass.set_index_buffer(panels_shared.indices.slice(..), wgpu::IndexFormat::Uint32);
		render_pass.set_vertex_buffer(0, panels_shared.vertices.slice(..));

		// Then render all panels simultaneously
		for panel in shared_window.panels.get_all() {
			self.render_panel(
				shared,
				panels_shared,
				wgpu_renderer,
				frame.surface_size,
				shared_window.monitor_geometry,
				&mut render_pass,
				panel,
			)?;
		}

		Ok(())
	}

	/// Renders a panel
	fn render_panel(
		&self,
		shared: &Shared,
		panels_shared: &PanelsRendererShared,
		wgpu_renderer: &WgpuRenderer,
		surface_size: PhysicalSize<u32>,
		window_geometry: Rect<i32, u32>,
		render_pass: &mut wgpu::RenderPass<'_>,
		panel: &mut Panel,
	) -> Result<(), app_error::AppError> {
		// Update the panel before drawing it
		match &mut panel.state {
			PanelState::None(_) => (),
			PanelState::Fade(state) => state.update(&shared.wgpu),
			#[expect(clippy::match_same_arms, reason = "We'll be changing them soon")]
			PanelState::Slide(_) => (),
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

		let render_pipeline = match panels_shared.render_pipelines.lock().entry(render_pipeline_id) {
			hash_map::Entry::Occupied(entry) => entry.get().share(),
			hash_map::Entry::Vacant(entry) => {
				let bind_group_layouts = match panel.state {
					PanelState::None(_) => {
						let none = panels_shared.none(&shared.wgpu);
						&[Some(&none.geometry_uniforms_bind_group_layout)] as &[_]
					},
					PanelState::Fade(_) => {
						let fade = panels_shared.fade(&shared.wgpu);
						&[
							Some(&fade.images.geometry_uniforms_bind_group_layout),
							Some(fade.images.image_bind_group_layout(&shared.wgpu)),
						]
					},
					PanelState::Slide(_) => {
						let slide = panels_shared.slide(&shared.wgpu);
						&[Some(&slide.geometry_uniforms_bind_group_layout)]
					},
				};

				let render_pipeline = self::create_render_pipeline(
					wgpu_renderer,
					&shared.wgpu,
					render_pipeline_id,
					bind_group_layouts,
					panel.state.shader(),
					self.msaa_samples,
				)
				.context("Unable to create render pipeline")?;

				entry.insert(Arc::new(render_pipeline)).share()
			},
		};

		// Bind the pipeline for the specific shader
		render_pass.set_pipeline(&render_pipeline);

		// Then render the panel
		Self::render_panel_geometries(shared, panels_shared, surface_size, window_geometry, render_pass, panel);

		Ok(())
	}

	/// Renders a panel's geometries
	fn render_panel_geometries(
		shared: &Shared,
		panels_shared: &PanelsRendererShared,
		surface_size: PhysicalSize<u32>,
		window_geometry: Rect<i32, u32>,
		render_pass: &mut wgpu::RenderPass<'_>,
		panel: &mut Panel,
	) {
		// Go through all geometries of the panel and render each one
		for panel_geometry in &mut panel.geometries {
			// If this geometry is outside our window, we can safely ignore it
			if !panel_geometry.intersects_window(window_geometry) {
				continue;
			}

			// Render the panel geometry
			Self::render_panel_geometry(
				&shared.wgpu,
				panels_shared,
				surface_size,
				&panel.state,
				window_geometry,
				panel_geometry,
				render_pass,
			);
		}
	}

	/// Renders a panel's geometry
	pub fn render_panel_geometry(
		wgpu: &Wgpu,
		shared: &PanelsRendererShared,
		surface_size: PhysicalSize<u32>,
		state: &PanelState,
		window_geometry: Rect<i32, u32>,
		panel_geometry: &mut PanelGeometry,
		render_pass: &mut wgpu::RenderPass<'_>,
	) {
		// Calculate the position matrix for the panel
		let pos_matrix = panel_geometry.pos_matrix(window_geometry, surface_size);

		match state {
			PanelState::None(state) => Self::render_panel_none_geometry(
				wgpu,
				render_pass,
				shared.none(wgpu),
				panel_geometry,
				pos_matrix,
				state,
			),
			PanelState::Fade(state) => Self::render_panel_fade_geometry(
				wgpu,
				render_pass,
				shared.fade(wgpu),
				panel_geometry,
				pos_matrix,
				state,
			),
			PanelState::Slide(state) => Self::render_panel_slide_geometry(
				wgpu,
				render_pass,
				shared.slide(wgpu),
				panel_geometry,
				pos_matrix,
				state,
			),
		}
	}

	/// Renders a panel none's geometry
	fn render_panel_none_geometry(
		wgpu: &Wgpu,
		render_pass: &mut wgpu::RenderPass<'_>,
		shared: &PanelNoneShared,
		panel_geometry: &mut PanelGeometry,
		pos_matrix: Transform3D<f32>,
		state: &PanelNoneState,
	) {
		let geometry_uniforms = panel_geometry.shared.none_or_insert_default().uniforms(wgpu, shared);

		Self::write_uniforms(wgpu, &geometry_uniforms.buffer, uniform::None {
			pos_matrix:       uniform::Matrix4x4(pos_matrix.to_arrays()),
			background_color: uniform::Vec4(state.background_color),
		});

		// Bind the geometry uniforms
		render_pass.set_bind_group(0, &geometry_uniforms.bind_group, &[]);

		render_pass.draw_indexed(0..6, 0, 0..1);
	}

	fn render_panel_fade_geometry(
		wgpu: &Wgpu,
		render_pass: &mut wgpu::RenderPass<'_>,
		shared: &PanelFadeShared,
		panel_geometry: &mut PanelGeometry,
		pos_matrix: Transform3D<f32>,
		state: &PanelFadeState,
	) {
		let p = state.progress_norm();
		let f = state.fade_duration_norm();

		// Full duration an image is on screen (including the fades)
		let d = 1.0 + 2.0 * f;

		let image_uniforms = |image: Option<&PanelFadeImage>, image_slot| -> uniform::fade::Image {
			let Some(image) = image else {
				return uniform::fade::Image {
					image_ratio: uniform::Vec2([1.0, 1.0]),
					progress:    0.0,
					alpha:       0.0,
				};
			};

			let progress = match image_slot {
				PanelFadeImageSlot::Prev => 1.0 - f32::max((f - p) / d, 0.0),
				PanelFadeImageSlot::Cur => (p + f) / d,
				PanelFadeImageSlot::Next => f32::max((p - 1.0 + f) / d, 0.0),
			};
			let progress = match image.swap_dir {
				true => 1.0 - progress,
				false => progress,
			};

			let p_stage = self::cmp_interval(p, f, 1.0 - f);
			let alpha = match p_stage {
				cmp::Ordering::Less => {
					let a = 0.5 + p / (2.0 * f);
					match image_slot {
						PanelFadeImageSlot::Prev => 1.0 - a,
						PanelFadeImageSlot::Cur => a,
						PanelFadeImageSlot::Next => 0.0,
					}
				},
				cmp::Ordering::Equal => match image_slot {
					PanelFadeImageSlot::Prev | PanelFadeImageSlot::Next => 0.0,
					PanelFadeImageSlot::Cur => 1.0,
				},
				cmp::Ordering::Greater => {
					let a = (p - (1.0 - f)) / (2.0 * f);
					match image_slot {
						PanelFadeImageSlot::Prev => 0.0,
						PanelFadeImageSlot::Cur => 1.0 - a,
						PanelFadeImageSlot::Next => a,
					}
				},
			};

			// Calculate the position matrix for the panel
			let image_size = image.texture_view.texture().size();
			let image_size = Vector2D::new(image_size.width, image_size.height);
			let image_ratio = panel_geometry.image_ratio(image_size);

			uniform::fade::Image {
				image_ratio: uniform::Vec2(image_ratio.into()),
				progress,
				alpha,
			}
		};

		let images = uniform::fade::Images {
			prev: image_uniforms(state.images().prev.as_ref(), PanelFadeImageSlot::Prev),
			cur:  image_uniforms(state.images().cur.as_ref(), PanelFadeImageSlot::Cur),
			next: image_uniforms(state.images().next.as_ref(), PanelFadeImageSlot::Next),
		};

		let geometry_uniforms = panel_geometry
			.shared
			.fade_or_insert_default()
			.images
			.uniforms(wgpu, &shared.images);
		let pos_matrix = uniform::Matrix4x4(pos_matrix.to_arrays());
		match state.shader() {
			PanelFadeShader::Basic => Self::write_uniforms(wgpu, &geometry_uniforms.buffer, uniform::fade::Basic {
				pos_matrix,
				images,
				_unused: [0; _],
			}),
			PanelFadeShader::Out { strength } =>
				Self::write_uniforms(wgpu, &geometry_uniforms.buffer, uniform::fade::Out {
					pos_matrix,
					images,
					strength,
					_unused: [0; _],
				}),
		}

		// Bind the geometry uniforms
		render_pass.set_bind_group(0, &geometry_uniforms.bind_group, &[]);

		// Bind the image uniforms
		let sampler = state.images().image_sampler(wgpu);
		render_pass.set_bind_group(1, state.images().bind_group(wgpu, sampler, &shared.images), &[]);

		render_pass.draw_indexed(0..6, 0, 0..1);
	}

	/// Renders a panel slide's geometry
	fn render_panel_slide_geometry(
		wgpu: &Wgpu,
		render_pass: &mut wgpu::RenderPass<'_>,
		shared: &PanelSlideShared,
		panel_geometry: &mut PanelGeometry,
		pos_matrix: Transform3D<f32>,
		_state: &PanelSlideState,
	) {
		let geometry_uniforms = panel_geometry.shared.slide_or_insert_default().uniforms(wgpu, shared);

		let pos_matrix = uniform::Matrix4x4(pos_matrix.to_arrays());
		Self::write_uniforms(wgpu, &geometry_uniforms.buffer, uniform::Slide { pos_matrix });

		// Bind the geometry uniforms
		render_pass.set_bind_group(0, &geometry_uniforms.bind_group, &[]);

		render_pass.draw_indexed(0..6, 0, 0..1);
	}

	/// Writes `uniforms` into `buffer`.
	fn write_uniforms<T>(wgpu: &Wgpu, buffer: &wgpu::Buffer, uniforms: T)
	where
		T: bytemuck::NoUninit,
	{
		wgpu.queue.write_buffer(buffer, 0, bytemuck::bytes_of(&uniforms));
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
	wgpu_renderer: &WgpuRenderer,
	wgpu: &Wgpu,
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
		format:     wgpu_renderer.surface_config().format,
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
	wgpu_renderer: &WgpuRenderer,
	wgpu: &Wgpu,
	size: PhysicalSize<u32>,
	msaa_samples: u32,
) -> wgpu::TextureView {
	let msaa_texture_extent = wgpu::Extent3d {
		width:                 size.width,
		height:                size.height,
		depth_or_array_layers: 1,
	};

	let surface_config = wgpu_renderer.surface_config();
	let msaa_frame_descriptor = wgpu::TextureDescriptor {
		label:           Some("zsw-panel-framebuffer-msaa"),
		size:            msaa_texture_extent,
		mip_level_count: 1,
		sample_count:    msaa_samples,
		dimension:       wgpu::TextureDimension::D2,
		format:          surface_config.format,
		usage:           wgpu::TextureUsages::RENDER_ATTACHMENT,
		view_formats:    &surface_config.view_formats,
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

/// Compares `value` to the interval `lhs..rhs`
pub fn cmp_interval(value: f32, lhs: f32, rhs: f32) -> cmp::Ordering {
	if value < lhs {
		return cmp::Ordering::Less;
	}
	if value > rhs {
		return cmp::Ordering::Greater;
	}

	cmp::Ordering::Equal
}
