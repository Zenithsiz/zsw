//! Panels renderer

pub mod uniform;
mod vertex;

pub use self::vertex::PanelVertex;

use {
	super::{
		Panel,
		PanelGeometry,
		PanelState,
		Panels,
		state::{
			PanelFadeState,
			PanelNoneState,
			PanelSlideState,
			fade::{PanelFadeImage, PanelFadeImageSlot, PanelFadeShared},
			none::PanelNoneShared,
			slide::{PanelSlideDir, PanelSlideShared},
		},
	},
	app_error::Context,
	core::cmp,
	euclid::default::{Transform3D, Vector2D},
	std::{
		borrow::Cow,
		collections::{HashMap, hash_map},
		sync::OnceLock,
	},
	wgpu::util::DeviceExt,
	winit::dpi::PhysicalSize,
	zsw_util::{AppError, Rect},
	zsw_wgpu::{FrameRender, WgpuRenderer},
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

	/// None
	none: OnceLock<PanelNoneShared>,

	/// Fade
	fade: OnceLock<PanelFadeShared>,

	/// Slide
	slide: OnceLock<PanelSlideShared>,
}

impl PanelsRenderer {
	/// Creates a new renderer for the panels
	pub fn new(wgpu_renderer: &WgpuRenderer, msaa_samples: u32) -> Result<Self, AppError> {
		// Create the framebuffer
		let msaa_framebuffer = self::create_msaa_framebuffer(wgpu_renderer, wgpu_renderer.surface_size, msaa_samples);

		// Create the index / vertex buffer
		let indices = self::create_indices(wgpu_renderer);
		let vertices = self::create_vertices(wgpu_renderer);

		Ok(Self {
			msaa_framebuffer,
			msaa_samples,
			render_pipelines: HashMap::new(),
			vertices,
			indices,
			none: OnceLock::new(),
			fade: OnceLock::new(),
			slide: OnceLock::new(),
		})
	}

	/// Resizes the buffer
	pub fn resize(&mut self, wgpu_renderer: &WgpuRenderer, size: PhysicalSize<u32>) {
		tracing::debug!("Resizing msaa framebuffer to {}x{}", size.width, size.height);
		self.msaa_framebuffer = self::create_msaa_framebuffer(wgpu_renderer, size, self.msaa_samples);
	}

	/// Renders a panel
	pub fn render(
		&mut self,
		wgpu_renderer: &WgpuRenderer,
		window_geometry: Rect<i32, u32>,
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
			self.render_panel(
				wgpu_renderer,
				frame.surface_size,
				window_geometry,
				&mut render_pass,
				panel,
			)?;
		}

		Ok(())
	}

	/// Renders a panel
	fn render_panel(
		&mut self,
		wgpu_renderer: &WgpuRenderer,

		surface_size: PhysicalSize<u32>,
		window_geometry: Rect<i32, u32>,
		render_pass: &mut wgpu::RenderPass<'_>,
		panel: &mut Panel,
	) -> Result<(), app_error::AppError> {
		// Update the panel before drawing it
		match &mut panel.state {
			PanelState::None(_) => (),
			PanelState::Fade(state) => state.update(wgpu_renderer),
			PanelState::Slide(state) => state.update(wgpu_renderer),
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
					PanelState::None(_) => {
						let none = self.none.get_or_init(|| PanelNoneShared::new(wgpu_renderer));
						&[Some(&none.geometry_uniforms_bind_group_layout)] as &[_]
					},
					PanelState::Fade(_) => {
						let fade = self.fade.get_or_init(|| PanelFadeShared::new(wgpu_renderer));
						&[
							Some(&fade.images.geometry_uniforms_bind_group_layout),
							Some(fade.images.image_bind_group_layout(wgpu_renderer)),
						]
					},
					PanelState::Slide(_) => {
						let slide = self.slide.get_or_init(|| PanelSlideShared::new(wgpu_renderer));
						&[
							Some(&slide.geometry_uniforms_bind_group_layout),
							Some(slide.image_bind_group_layout(wgpu_renderer)),
						]
					},
				};

				let render_pipeline = self::create_render_pipeline(
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
		self.render_panel_geometries(wgpu_renderer, surface_size, window_geometry, render_pass, panel);

		Ok(())
	}

	/// Renders a panel's geometries
	fn render_panel_geometries(
		&self,
		wgpu_renderer: &WgpuRenderer,
		surface_size: PhysicalSize<u32>,
		window_geometry: Rect<i32, u32>,
		render_pass: &mut wgpu::RenderPass<'_>,
		panel: &mut Panel,
	) {
		// Go through all geometries of the panel and render each one
		for panel_geometry in &mut panel.geometries {
			// If this geometry is outside our window, we can safely ignore it
			if !panel_geometry.rect.intersects_window(window_geometry) {
				continue;
			}

			// Render the panel geometry
			self.render_panel_geometry(
				wgpu_renderer,
				surface_size,
				&mut panel.state,
				window_geometry,
				panel_geometry,
				render_pass,
			);
		}
	}

	/// Renders a panel's geometry
	pub fn render_panel_geometry(
		&self,
		wgpu_renderer: &WgpuRenderer,
		surface_size: PhysicalSize<u32>,
		state: &mut PanelState,
		window_geometry: Rect<i32, u32>,
		panel_geometry: &mut PanelGeometry,
		render_pass: &mut wgpu::RenderPass<'_>,
	) {
		match state {
			PanelState::None(state) => self.render_panel_none_geometry(
				wgpu_renderer,
				render_pass,
				panel_geometry,
				panel_geometry.rect.pos_matrix(window_geometry, surface_size),
				state,
			),
			PanelState::Fade(state) => self.render_panel_fade_geometry(
				wgpu_renderer,
				render_pass,
				panel_geometry,
				panel_geometry.rect.pos_matrix(window_geometry, surface_size),
				state,
			),
			PanelState::Slide(state) => self.render_panel_slide_geometry(
				wgpu_renderer,
				render_pass,
				panel_geometry,
				panel_geometry.rect.pos_matrix(window_geometry, surface_size),
				state,
			),
		}
	}

	/// Renders a panel none's geometry
	fn render_panel_none_geometry(
		&self,
		wgpu_renderer: &WgpuRenderer,
		render_pass: &mut wgpu::RenderPass<'_>,
		panel_geometry: &mut PanelGeometry,
		pos_matrix: Transform3D<f32>,
		state: &PanelNoneState,
	) {
		let geometry_uniforms = panel_geometry.shared.none_or_insert_default().uniforms(
			wgpu_renderer,
			self.none.get_or_init(|| PanelNoneShared::new(wgpu_renderer)),
		);

		Self::write_uniforms(wgpu_renderer, &geometry_uniforms.buffer, uniform::None {
			pos_matrix:       uniform::Matrix4x4(pos_matrix.to_arrays()),
			background_color: uniform::Vec4(state.background_color),
		});

		// Bind the geometry uniforms
		render_pass.set_bind_group(0, &geometry_uniforms.bind_group, &[]);

		render_pass.draw_indexed(0..6, 0, 0..1);
	}

	fn render_panel_fade_geometry(
		&self,
		wgpu_renderer: &WgpuRenderer,
		render_pass: &mut wgpu::RenderPass<'_>,
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
			let image_ratio = panel_geometry.rect.image_ratio(image_size);

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

		let shared = self.fade.get_or_init(|| PanelFadeShared::new(wgpu_renderer));
		let geometry_uniforms = panel_geometry
			.shared
			.fade_or_insert_default()
			.images
			.uniforms(wgpu_renderer, &shared.images);
		let pos_matrix = uniform::Matrix4x4(pos_matrix.to_arrays());
		match state.shader() {
			PanelFadeShader::Basic =>
				Self::write_uniforms(wgpu_renderer, &geometry_uniforms.buffer, uniform::fade::Basic {
					pos_matrix,
					images,
					_unused: [0; _],
				}),
			PanelFadeShader::Out { strength } =>
				Self::write_uniforms(wgpu_renderer, &geometry_uniforms.buffer, uniform::fade::Out {
					pos_matrix,
					images,
					strength,
					_unused: [0; _],
				}),
		}

		// Bind the geometry uniforms
		render_pass.set_bind_group(0, &geometry_uniforms.bind_group, &[]);

		// Bind the image uniforms
		let sampler = state.images().image_sampler(wgpu_renderer);
		render_pass.set_bind_group(1, state.images().bind_group(wgpu_renderer, sampler, &shared.images), &[
		]);

		render_pass.draw_indexed(0..6, 0, 0..1);
	}

	/// Renders a panel slide's geometry
	fn render_panel_slide_geometry(
		&self,
		wgpu_renderer: &WgpuRenderer,
		render_pass: &mut wgpu::RenderPass<'_>,
		panel_geometry: &mut PanelGeometry,
		pos_matrix: Transform3D<f32>,
		state: &mut PanelSlideState,
	) {
		let mut missing_images = true;
		let mut cur_global_offset = 0.0;

		let img_offset = state.progress().div_duration_floor(state.duration()) as usize;
		// TODO: Deduplicate this with below
		let local_offset = match state.images().nth(img_offset) {
			Some(image) => {
				let image_size = image.texture_view.texture().size();
				let image_size = Vector2D::new(image_size.width, image_size.height);
				let image_ratio = panel_geometry.rect.image_ratio(image_size);

				let ratio = match state.dir().is_horizontal() {
					true => image_ratio.y / image_ratio.x,
					false => image_ratio.x / image_ratio.y,
				};

				let offset_abs = state.progress().as_secs_f32() / state.duration().as_secs_f32() - img_offset as f32;
				offset_abs * ratio * 2.0
			},
			None => 0.0,
		};

		for (image_idx, image) in state.images().enumerate().skip(img_offset) {
			// Calculate the position matrix for the panel
			let image_size = image.texture_view.texture().size();
			let image_size = Vector2D::new(image_size.width, image_size.height);
			let image_ratio = panel_geometry.rect.image_ratio(image_size);

			let offset_abs = cur_global_offset - local_offset;
			if offset_abs > 2.0 {
				missing_images = false;
				break;
			}

			// Bind the geometry uniforms
			let shared = self.slide.get_or_init(|| PanelSlideShared::new(wgpu_renderer));
			let geometry_uniforms =
				panel_geometry
					.shared
					.slide_or_insert_default()
					.uniforms(wgpu_renderer, shared, image_idx);
			render_pass.set_bind_group(0, &geometry_uniforms.bind_group, &[]);

			let ratio = match state.dir().is_horizontal() {
				true => image_ratio.y / image_ratio.x,
				false => image_ratio.x / image_ratio.y,
			};

			// TODO: This should be baked into the position matrix instead.
			let offset: Vector2D<f32> = match state.dir() {
				PanelSlideDir::LeftRight => euclid::vec2(offset_abs, 0.0),
				PanelSlideDir::RightLeft => euclid::vec2(2.0 * (1.0 - ratio) - offset_abs, 0.0),
				PanelSlideDir::UpDown => euclid::vec2(0.0, offset_abs),
				PanelSlideDir::DownUp => euclid::vec2(0.0, 2.0 * (1.0 - ratio) - offset_abs),
			};

			Self::write_uniforms(wgpu_renderer, &geometry_uniforms.buffer, uniform::Slide {
				pos_matrix:  uniform::Matrix4x4(pos_matrix.to_arrays()),
				image_ratio: uniform::Vec2(image_ratio.into()),
				offset:      uniform::Vec2(offset.to_array()),
			});

			cur_global_offset += ratio * 2.0;

			let sampler = state.image_sampler(wgpu_renderer);
			render_pass.set_bind_group(1, image.bind_group(wgpu_renderer, sampler, shared), &[]);

			render_pass.draw_indexed(0..6, 0, 0..1);
		}

		if missing_images {
			state.load_next(wgpu_renderer);
		}
	}

	/// Writes `uniforms` into `buffer`.
	fn write_uniforms<T>(wgpu_renderer: &WgpuRenderer, buffer: &wgpu::Buffer, uniforms: T)
	where
		T: bytemuck::NoUninit,
	{
		wgpu_renderer
			.queue
			.write_buffer(buffer, 0, bytemuck::bytes_of(&uniforms));
	}
}

/// Creates the vertices
fn create_vertices(wgpu_renderer: &WgpuRenderer) -> wgpu::Buffer {
	let descriptor = wgpu::util::BufferInitDescriptor {
		label:    Some("zsw-panel-vertex-buffer"),
		contents: bytemuck::cast_slice(&PanelVertex::QUAD),
		usage:    wgpu::BufferUsages::VERTEX,
	};

	wgpu_renderer.device.create_buffer_init(&descriptor)
}

/// Creates the indices
fn create_indices(wgpu_renderer: &WgpuRenderer) -> wgpu::Buffer {
	const INDICES: [u32; 6] = [0, 1, 3, 0, 3, 2];
	let descriptor = wgpu::util::BufferInitDescriptor {
		label:    Some("zsw-panel-index-buffer"),
		contents: bytemuck::cast_slice(&INDICES),
		usage:    wgpu::BufferUsages::INDEX,
	};

	wgpu_renderer.device.create_buffer_init(&descriptor)
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
	let shader = wgpu_renderer.device.create_shader_module(shader_descriptor);

	// Create the pipeline layout
	let render_pipeline_layout_descriptor = wgpu::PipelineLayoutDescriptor {
		label: Some(&format!(
			"zsw-panel-render-pipeline[name={render_pipeline_name:?}]-layout"
		)),
		bind_group_layouts,
		immediate_size: 0,
	};
	let render_pipeline_layout = wgpu_renderer
		.device
		.create_pipeline_layout(&render_pipeline_layout_descriptor);

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

	Ok(wgpu_renderer.device.create_render_pipeline(&render_pipeline_descriptor))
}

/// Creates the msaa framebuffer
fn create_msaa_framebuffer(
	wgpu_renderer: &WgpuRenderer,
	size: PhysicalSize<u32>,
	msaa_samples: u32,
) -> wgpu::TextureView {
	let msaa_texture_extent = wgpu::Extent3d {
		width:                 size.width,
		height:                size.height,
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

	wgpu_renderer
		.device
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
