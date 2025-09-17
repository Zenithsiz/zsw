//! Panels renderer

// Modules
pub mod uniform;
mod vertex;

// Exports
pub use self::vertex::PanelVertex;

// Imports
use {
	super::{
		Panel,
		PanelState,
		Panels,
		geometry::{
			PanelGeometryFadeUniforms,
			PanelGeometryNoneUniforms,
			PanelGeometrySlideUniforms,
			PanelGeometryUniforms,
		},
		state::{
			PanelFadeState,
			PanelNoneState,
			PanelSlideState,
			fade::{PanelFadeImageSlot, PanelFadeImagesShared},
			none::PanelNoneShared,
			slide::PanelSlideShared,
		},
	},
	crate::{
		display::DisplayGeometry,
		metrics::{self, Metrics},
		panel::PanelGeometry,
		time,
	},
	app_error::Context,
	cgmath::Vector2,
	futures::{StreamExt, stream::FuturesUnordered},
	itertools::Itertools,
	std::{
		borrow::Cow,
		collections::{HashMap, hash_map},
		sync::Arc,
	},
	tokio::sync::Mutex,
	wgpu::util::DeviceExt,
	winit::{dpi::PhysicalSize, window::Window},
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
	none: PanelNoneShared,

	/// Fade
	fade: PanelFadeImagesShared,

	/// Slide
	slide: PanelSlideShared,
}

impl PanelsRendererShared {
	/// Creates new layouts for the panels renderer
	pub fn new(wgpu: &Wgpu) -> Self {
		// Create the index / vertex buffer
		let indices = self::create_indices(wgpu);
		let vertices = self::create_vertices(wgpu);

		let none = PanelNoneShared::new(wgpu);
		let fade = PanelFadeImagesShared::new(wgpu);
		let slide = PanelSlideShared::new(wgpu);

		Self {
			render_pipelines: Mutex::new(HashMap::new()),
			vertices,
			indices,
			none,
			fade,
			slide,
		}
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
	pub async fn render(
		&self,
		frame: &mut FrameRender,
		wgpu_renderer: &WgpuRenderer,
		wgpu: &Wgpu,
		shared: &PanelsRendererShared,
		panels: &Panels,
		metrics: &Metrics,
		window: &Window,
		window_geometry: Rect<i32, u32>,
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
		};
		#[time(create_render_pass)]
		let mut render_pass = frame.encoder.begin_render_pass(&render_pass_descriptor);

		// Set our shared indices and vertices
		render_pass.set_index_buffer(shared.indices.slice(..), wgpu::IndexFormat::Uint32);
		render_pass.set_vertex_buffer(0, shared.vertices.slice(..));

		// Then render all panels simultaneously
		#[time(lock_panels)]
		let panels = panels.get_all().await;
		let mut panels = panels
			.iter()
			.enumerate()
			.map(|(panel_idx, panel)| async move { (panel_idx, panel.lock().await) })
			.collect::<FuturesUnordered<_>>();
		let mut panels_metrics = HashMap::new();
		while let Some((panel_idx, mut panel)) = panels.next().await {
			self.render_panel(
				wgpu,
				shared,
				wgpu_renderer,
				frame.surface_size,
				window,
				window_geometry,
				&mut render_pass,
				panel_idx,
				&mut panel,
				&mut panels_metrics,
			)
			.await?;
		}

		metrics
			.render_panels_frame_times(window.id())
			.await
			.add(metrics::RenderPanelsFrameTime {
				create_render_pass,
				lock_panels,
				panels: panels_metrics,
			});

		Ok(())
	}

	/// Renders a panel
	async fn render_panel(
		&self,
		wgpu: &Wgpu,
		shared: &PanelsRendererShared,
		wgpu_renderer: &WgpuRenderer,
		surface_size: PhysicalSize<u32>,
		window: &Window,
		window_geometry: Rect<i32, u32>,
		render_pass: &mut wgpu::RenderPass<'_>,
		panel_idx: usize,
		panel: &mut Panel,
		panels_metrics: &mut HashMap<usize, metrics::RenderPanelFrameTime>,
	) -> Result<(), app_error::AppError> {
		// Update the panel before drawing it
		#[time(update_panel)]
		let () = match &mut panel.state {
			PanelState::None(_) => (),
			PanelState::Fade(state) => state.update(wgpu).await,
			#[expect(clippy::match_same_arms, reason = "We'll be changing them soon")]
			PanelState::Slide(_) => (),
		};

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
				PanelFadeShader::White { .. } => RenderPipelineFadeId::White,
				PanelFadeShader::Out { .. } => RenderPipelineFadeId::Out,
				PanelFadeShader::In { .. } => RenderPipelineFadeId::In,
			}),
			PanelState::Slide(state) => RenderPipelineId::Slide(match state.shader() {
				PanelSlideShader::Basic => RenderPipelineSlideId::Basic,
			}),
		};

		#[time(create_render_pipeline)]
		let render_pipeline = match shared.render_pipelines.lock().await.entry(render_pipeline_id) {
			hash_map::Entry::Occupied(entry) => Arc::clone(entry.get()),
			hash_map::Entry::Vacant(entry) => {
				let bind_group_layouts = match panel.state {
					PanelState::None(_) => &[&shared.none.geometry_uniforms_bind_group_layout] as &[_],
					PanelState::Fade(_) => &[
						&shared.fade.geometry_uniforms_bind_group_layout,
						shared.fade.image_bind_group_layout(wgpu).await,
					],
					PanelState::Slide(_) => &[&shared.slide.geometry_uniforms_bind_group_layout],
				};

				let render_pipeline = self::create_render_pipeline(
					wgpu_renderer,
					wgpu,
					render_pipeline_id,
					bind_group_layouts,
					panel.state.shader(),
					self.msaa_samples,
				)
				.context("Unable to create render pipeline")?;

				Arc::clone(entry.insert(Arc::new(render_pipeline)))
			},
		};

		// Bind the pipeline for the specific shader
		render_pass.set_pipeline(&render_pipeline);

		let panel_metrics = panels_metrics
			.entry(panel_idx)
			.or_insert_with(|| metrics::RenderPanelFrameTime {
				update_panel,
				create_render_pipeline,
				geometries: HashMap::new(),
			});

		// Then render the panel
		Self::render_panel_geometries(
			wgpu,
			shared,
			surface_size,
			window,
			window_geometry,
			render_pass,
			panel,
			panel_metrics,
		)
		.await;

		Ok(())
	}

	/// Renders a panel's geometries
	async fn render_panel_geometries(
		wgpu: &Wgpu,
		shared: &PanelsRendererShared,
		surface_size: PhysicalSize<u32>,
		window: &Window,
		window_geometry: Rect<i32, u32>,
		render_pass: &mut wgpu::RenderPass<'_>,
		panel: &mut Panel,
		panel_metrics: &mut metrics::RenderPanelFrameTime,
	) {
		// The display might have changed asynchronously from the panel geometries,
		// so resize it to ensure we have a panel geometry for each display geometry.
		let display = panel.display.read().await;
		panel
			.geometries
			.resize_with(display.geometries.len(), PanelGeometry::new);

		// Go through all geometries of the panel display and render each one
		for (geometry_idx, (display_geometry, panel_geometry)) in
			display.geometries.iter().zip_eq(&mut panel.geometries).enumerate()
		{
			// If this geometry is outside our window, we can safely ignore it
			if !display_geometry.intersects_window(window_geometry) {
				continue;
			}

			// Render the panel geometry
			let geometry_uniforms = panel_geometry.uniforms.entry(window.id()).or_default();
			let geometry_metrics = Self::render_panel_geometry(
				wgpu,
				shared,
				surface_size,
				&panel.state,
				window_geometry,
				display_geometry,
				geometry_uniforms,
				render_pass,
			)
			.await;

			_ = panel_metrics.geometries.insert(geometry_idx, geometry_metrics);
		}
	}

	/// Renders a panel's geometry
	pub async fn render_panel_geometry(
		wgpu: &Wgpu,
		shared: &PanelsRendererShared,
		surface_size: PhysicalSize<u32>,
		panel_state: &PanelState,
		window_geometry: Rect<i32, u32>,
		display_geometry: &DisplayGeometry,
		geometry_uniforms: &mut PanelGeometryUniforms,
		render_pass: &mut wgpu::RenderPass<'_>,
	) -> metrics::RenderPanelGeometryFrameTime {
		// Calculate the position matrix for the panel
		let pos_matrix = display_geometry.pos_matrix(window_geometry, surface_size);
		let pos_matrix = uniform::Matrix4x4(pos_matrix.into());

		match panel_state {
			PanelState::None(panel_state) => Self::render_panel_none_geometry(
				wgpu,
				render_pass,
				pos_matrix,
				geometry_uniforms
					.none(wgpu, &shared.none.geometry_uniforms_bind_group_layout)
					.await,
				panel_state,
			)
			.into(),
			PanelState::Fade(panel_state) => Self::render_panel_fade_geometry(
				wgpu,
				shared,
				display_geometry,
				render_pass,
				pos_matrix,
				&mut geometry_uniforms.fade,
				panel_state,
			)
			.await
			.into(),
			PanelState::Slide(panel_state) => Self::render_panel_slide_geometry(
				wgpu,
				render_pass,
				pos_matrix,
				geometry_uniforms
					.slide(wgpu, &shared.slide.geometry_uniforms_bind_group_layout)
					.await,
				panel_state,
			)
			.into(),
		}
	}

	/// Renders a panel none's geometry
	fn render_panel_none_geometry(
		wgpu: &Wgpu,
		render_pass: &mut wgpu::RenderPass<'_>,
		pos_matrix: uniform::Matrix4x4,
		geometry_uniforms: &PanelGeometryNoneUniforms,
		panel_state: &PanelNoneState,
	) -> metrics::RenderPanelGeometryNoneFrameTime {
		#[time(write_uniforms)]
		let () = Self::write_uniforms(wgpu, &geometry_uniforms.buffer, uniform::None {
			pos_matrix,
			background_color: uniform::Vec4(panel_state.background_color),
		});

		// Bind the geometry uniforms
		render_pass.set_bind_group(0, &geometry_uniforms.bind_group, &[]);

		#[time(draw)]
		render_pass.draw_indexed(0..6, 0, 0..1);

		metrics::RenderPanelGeometryNoneFrameTime { write_uniforms, draw }
	}

	async fn render_panel_fade_geometry(
		wgpu: &Wgpu,
		shared: &PanelsRendererShared,
		display_geometry: &DisplayGeometry,
		render_pass: &mut wgpu::RenderPass<'_>,
		pos_matrix: uniform::Matrix4x4,
		geometry_uniforms: &mut PanelGeometryFadeUniforms,
		panel_state: &PanelFadeState,
	) -> metrics::RenderPanelGeometryFadeFrameTime {
		let p = panel_state.progress_norm();
		let f = panel_state.fade_duration_norm();

		// Full duration an image is on screen (including the fades)
		let d = 1.0 + 2.0 * f;

		let mut image_metrics = HashMap::new();
		for (panel_image_slot, panel_image) in panel_state.images().iter() {
			let geometry_uniforms =
				geometry_uniforms.image(wgpu, &shared.fade.geometry_uniforms_bind_group_layout, panel_image_slot);

			let progress = match panel_image_slot {
				PanelFadeImageSlot::Prev => 1.0 - f32::max((f - p) / d, 0.0),
				PanelFadeImageSlot::Cur => (p + f) / d,
				PanelFadeImageSlot::Next => f32::max((p - 1.0 + f) / d, 0.0),
			};
			let progress = match panel_image.swap_dir {
				true => 1.0 - progress,
				false => progress,
			};

			let alpha_prev = 0.5 * f32::clamp(1.0 - p / f, 0.0, 1.0);
			let alpha_next = 0.5 * f32::clamp(1.0 - (1.0 - p) / f, 0.0, 1.0);
			let alpha_cur = 1.0 - f32::max(alpha_prev, alpha_next);
			let alpha = match panel_image_slot {
				PanelFadeImageSlot::Prev => alpha_prev,
				PanelFadeImageSlot::Cur => alpha_cur,
				PanelFadeImageSlot::Next => alpha_next,
			};

			// If the alpha is 0, we can skip this image
			if alpha == 0.0 {
				continue;
			}

			// Calculate the position matrix for the panel
			let image_size = panel_image.texture_view.texture().size();
			let image_size = Vector2::new(image_size.width, image_size.height);
			let image_ratio = display_geometry.image_ratio(image_size);

			#[time(write_uniforms)]
			let () = match panel_state.shader() {
				PanelFadeShader::Basic => Self::write_uniforms(wgpu, &geometry_uniforms.buffer, uniform::FadeBasic {
					pos_matrix,
					image_ratio: uniform::Vec2(image_ratio.into()),
					progress,
					alpha,
				}),
				PanelFadeShader::White { strength } =>
					Self::write_uniforms(wgpu, &geometry_uniforms.buffer, uniform::FadeWhite {
						pos_matrix,
						image_ratio: uniform::Vec2(image_ratio.into()),
						progress,
						alpha,
						mix_strength: strength * 4.0 * f32::max(alpha_cur * alpha_prev, alpha_cur * alpha_next),
						_unused: [0; _],
					}),
				PanelFadeShader::Out { strength } =>
					Self::write_uniforms(wgpu, &geometry_uniforms.buffer, uniform::FadeOut {
						pos_matrix,
						image_ratio: uniform::Vec2(image_ratio.into()),
						progress,
						alpha,
						strength,
						_unused: [0; _],
					}),
				PanelFadeShader::In { strength } =>
					Self::write_uniforms(wgpu, &geometry_uniforms.buffer, uniform::FadeIn {
						pos_matrix,
						image_ratio: uniform::Vec2(image_ratio.into()),
						progress,
						alpha,
						strength,
						_unused: [0; _],
					}),
			};

			// Bind the geometry uniforms
			render_pass.set_bind_group(0, &geometry_uniforms.bind_group, &[]);

			// Bind the image uniforms
			let sampler = panel_state.images().image_sampler(wgpu).await;
			render_pass.set_bind_group(1, panel_image.bind_group(wgpu, sampler, &shared.fade).await, &[]);

			#[time(draw)]
			render_pass.draw_indexed(0..6, 0, 0..1);

			_ = image_metrics.insert(panel_image_slot, metrics::RenderPanelGeometryFadeImageFrameTime {
				write_uniforms,
				draw,
			});
		}

		metrics::RenderPanelGeometryFadeFrameTime { images: image_metrics }
	}

	/// Renders a panel slide's geometry
	fn render_panel_slide_geometry(
		wgpu: &Wgpu,
		render_pass: &mut wgpu::RenderPass<'_>,
		pos_matrix: uniform::Matrix4x4,
		geometry_uniforms: &PanelGeometrySlideUniforms,
		_panel_state: &PanelSlideState,
	) -> metrics::RenderPanelGeometrySlideFrameTime {
		#[time(write_uniforms)]
		let () = Self::write_uniforms(wgpu, &geometry_uniforms.buffer, uniform::Slide { pos_matrix });

		// Bind the geometry uniforms
		render_pass.set_bind_group(0, &geometry_uniforms.bind_group, &[]);

		#[time(draw)]
		render_pass.draw_indexed(0..6, 0, 0..1);

		metrics::RenderPanelGeometrySlideFrameTime { write_uniforms, draw }
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
	White,
	Out,
	In,
}

impl RenderPipelineFadeId {
	/// Returns this pipeline's name
	pub fn name(self) -> &'static str {
		match self {
			Self::Basic => "fade-basic",
			Self::White => "fade-white",
			Self::Out => "fade-out",
			Self::In => "fade-in",
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
	bind_group_layouts: &[&wgpu::BindGroupLayout],
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
		push_constant_ranges: &[],
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

		vertex:        wgpu::VertexState {
			module:              &shader,
			entry_point:         Some("vs_main"),
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
			count: msaa_samples,
			mask: u64::MAX,
			alpha_to_coverage_enabled: false,
		},
		fragment:      Some(wgpu::FragmentState {
			module:              &shader,
			entry_point:         Some("fs_main"),
			targets:             &color_targets,
			compilation_options: wgpu::PipelineCompilationOptions::default(),
		}),
		multiview:     None,
		cache:         None,
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
	White { strength: f32 },
	Out { strength: f32 },
	In { strength: f32 },
}

impl PanelFadeShader {
	/// Returns this shader's name
	pub fn name(self) -> &'static str {
		match self {
			Self::Basic => "Fade",
			Self::White { .. } => "Fade white",
			Self::Out { .. } => "Fade out",
			Self::In { .. } => "Fade in",
		}
	}

	/// Returns this shader's module as json
	pub fn module_json(self) -> &'static str {
		match self {
			Self::Basic => include_str!(concat!(env!("OUT_DIR"), "/shaders/panels/fade.json")),
			Self::White { .. } => include_str!(concat!(env!("OUT_DIR"), "/shaders/panels/fade-white.json")),
			Self::Out { .. } => include_str!(concat!(env!("OUT_DIR"), "/shaders/panels/fade-out.json")),
			Self::In { .. } => include_str!(concat!(env!("OUT_DIR"), "/shaders/panels/fade-in.json")),
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
