//! Panels renderer

// Modules
mod uniform;
mod vertex;

// Exports
pub use self::{uniform::MAX_UNIFORM_SIZE, vertex::PanelVertex};

// Imports
use {
	super::{
		PanelState,
		Panels,
		geometry::{PanelGeometryFadeImageUniforms, PanelGeometryNoneUniforms, PanelGeometrySlideUniforms},
		state::fade::{PanelFadeImagesShared, images::PanelFadeImageSlot},
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

	// TODO: Move these to the respective shader shared types
	/// Uniforms none bind group layout
	none_uniforms_bind_group_layout: wgpu::BindGroupLayout,

	/// Uniforms fade bind group layout
	fade_uniforms_bind_group_layout: wgpu::BindGroupLayout,

	/// Uniforms slide bind group layout
	slide_uniforms_bind_group_layout: wgpu::BindGroupLayout,

	/// Fade
	fade: PanelFadeImagesShared,
}

impl PanelsRendererShared {
	/// Creates new layouts for the panels renderer
	pub fn new(wgpu: &Wgpu) -> Self {
		// Create the index / vertex buffer
		let indices = self::create_indices(wgpu);
		let vertices = self::create_vertices(wgpu);

		let none_uniforms_bind_group_layout = self::create_none_uniforms_bind_group_layout(wgpu);
		let fade_uniforms_bind_group_layout = self::create_fade_uniforms_bind_group_layout(wgpu);
		let slide_uniforms_bind_group_layout = self::create_slide_uniforms_bind_group_layout(wgpu);

		Self {
			render_pipelines: Mutex::new(HashMap::new()),
			vertices,
			indices,
			none_uniforms_bind_group_layout,
			fade_uniforms_bind_group_layout,
			slide_uniforms_bind_group_layout,
			fade: PanelFadeImagesShared::default(),
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
	#[expect(clippy::too_many_lines, reason = "TODO: Split it up")]
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
			let panel = &mut *panel;

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
				continue;
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
						PanelState::None(_) => &[&shared.none_uniforms_bind_group_layout] as &[_],
						PanelState::Fade(_) => &[
							&shared.fade_uniforms_bind_group_layout,
							shared.fade.image_bind_group_layout(wgpu).await,
						],
						PanelState::Slide(_) => &[&shared.slide_uniforms_bind_group_layout],
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

			// The display might have changed asynchronously from the panel geometries,
			// so resize it to ensure we have a panel geometry for each display geometry.
			let display = panel.display.read().await;
			panel
				.geometries
				.resize_with(display.geometries.len(), PanelGeometry::new);

			let panel_metrics = panels_metrics
				.entry(panel_idx)
				.or_insert_with(|| metrics::RenderPanelFrameTime {
					update_panel,
					create_render_pipeline,
					geometries: HashMap::new(),
				});
			for (geometry_idx, (display_geometry, panel_geometry)) in
				display.geometries.iter().zip_eq(&mut panel.geometries).enumerate()
			{
				// If this geometry is outside our window, we can safely ignore it
				if !display_geometry.intersects_window(window_geometry) {
					continue;
				}

				// Render the panel geometry
				let geometry_metrics = Self::render_panel_geometry(
					wgpu,
					shared,
					frame.surface_size,
					&panel.state,
					window_geometry,
					window,
					display_geometry,
					panel_geometry,
					&mut render_pass,
				)
				.await;

				_ = panel_metrics.geometries.insert(geometry_idx, geometry_metrics);
			}
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

	/// Renders a panel's geometry
	#[expect(clippy::too_many_lines, reason = "TODO: Split it up")]
	pub async fn render_panel_geometry(
		wgpu: &Wgpu,
		shared: &PanelsRendererShared,
		surface_size: PhysicalSize<u32>,
		panel_state: &PanelState,
		window_geometry: Rect<i32, u32>,
		window: &Window,
		display_geometry: &DisplayGeometry,
		panel_geometry: &mut PanelGeometry,
		render_pass: &mut wgpu::RenderPass<'_>,
	) -> metrics::RenderPanelGeometryFrameTime {
		// Calculate the position matrix for the panel
		let pos_matrix = display_geometry.pos_matrix(window_geometry, surface_size);
		let pos_matrix = uniform::Matrix4x4(pos_matrix.into());

		let geometry_uniforms = panel_geometry.uniforms.entry(window.id()).or_default();
		match panel_state {
			PanelState::None(panel_state) => {
				let geometry_uniforms = geometry_uniforms
					.none
					.get_or_init(async || {
						self::create_geometry_none_uniforms(wgpu, &shared.none_uniforms_bind_group_layout)
					})
					.await;

				#[time(write_uniforms)]
				let () = Self::write_uniforms(wgpu, &geometry_uniforms.buffer, uniform::None {
					pos_matrix,
					background_color: uniform::Vec4(panel_state.background_color),
				});

				// Bind the geometry uniforms
				render_pass.set_bind_group(0, &geometry_uniforms.bind_group, &[]);

				#[time(draw)]
				render_pass.draw_indexed(0..6, 0, 0..1);

				metrics::RenderPanelGeometryFrameTime::None(metrics::RenderPanelGeometryNoneFrameTime {
					write_uniforms,
					draw,
				})
			},
			PanelState::Fade(panel_state) => {
				let p = panel_state.progress_norm();
				let f = panel_state.fade_duration_norm();

				// Full duration an image is on screen (including the fades)
				let d = 1.0 + 2.0 * f;

				let mut image_metrics = HashMap::new();
				for (panel_image_slot, panel_image) in panel_state.images().iter() {
					let geometry_uniforms =
						geometry_uniforms
							.fade
							.images
							.entry(panel_image_slot)
							.or_insert_with(|| {
								self::create_geometry_fade_image_uniforms(wgpu, &shared.fade_uniforms_bind_group_layout)
							});

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
					// TODO: If alpha of a previous layer is 1, should we also skip it?
					//       Right now that's fine, since with alpha 1, we always cover the
					//       whole screen, but that's not guaranteed in the future.
					if alpha == 0.0 {
						continue;
					}

					// Calculate the position matrix for the panel
					let image_size = panel_image.texture_view.texture().size();
					let image_size = Vector2::new(image_size.width, image_size.height);
					let image_ratio = display_geometry.image_ratio(image_size);

					#[time(write_uniforms)]
					let () = match panel_state.shader() {
						PanelFadeShader::Basic =>
							Self::write_uniforms(wgpu, &geometry_uniforms.buffer, uniform::FadeBasic {
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

				metrics::RenderPanelGeometryFrameTime::Fade(metrics::RenderPanelGeometryFadeFrameTime {
					images: image_metrics,
				})
			},
			PanelState::Slide(_panel_state) => {
				let geometry_uniforms = geometry_uniforms
					.slide
					.get_or_init(async || {
						self::create_geometry_slide_uniforms(wgpu, &shared.slide_uniforms_bind_group_layout)
					})
					.await;

				#[time(write_uniforms)]
				let () = Self::write_uniforms(wgpu, &geometry_uniforms.buffer, uniform::Slide { pos_matrix });

				// Bind the geometry uniforms
				render_pass.set_bind_group(0, &geometry_uniforms.bind_group, &[]);

				#[time(draw)]
				render_pass.draw_indexed(0..6, 0, 0..1);

				metrics::RenderPanelGeometryFrameTime::Slide(metrics::RenderPanelGeometrySlideFrameTime {
					write_uniforms,
					draw,
				})
			},
		}
	}

	/// Writes `uniforms` into `buffer`.
	fn write_uniforms<T>(wgpu: &Wgpu, buffer: &wgpu::Buffer, uniforms: T)
	where
		T: bytemuck::NoUninit,
	{
		wgpu.queue.write_buffer(buffer, 0, bytemuck::bytes_of(&uniforms));
	}
}

/// Creates the panel none geometry uniforms
fn create_geometry_none_uniforms(wgpu: &Wgpu, layout: &wgpu::BindGroupLayout) -> PanelGeometryNoneUniforms {
	// Create the uniforms
	let buffer_descriptor = wgpu::BufferDescriptor {
		label:              Some("zsw-panel-none-geometry-uniforms-buffer"),
		usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
		size:               u64::try_from(MAX_UNIFORM_SIZE).expect("Maximum uniform size didn't fit into a `u64`"),
		mapped_at_creation: false,
	};
	let buffer = wgpu.device.create_buffer(&buffer_descriptor);

	// Create the uniform bind group
	let bind_group_descriptor = wgpu::BindGroupDescriptor {
		label: Some("zsw-panel-none-geometry-uniforms-bind-group"),
		layout,
		entries: &[wgpu::BindGroupEntry {
			binding:  0,
			resource: buffer.as_entire_binding(),
		}],
	};
	let bind_group = wgpu.device.create_bind_group(&bind_group_descriptor);

	PanelGeometryNoneUniforms { buffer, bind_group }
}

/// Creates the panel fade image geometry uniforms
fn create_geometry_fade_image_uniforms(wgpu: &Wgpu, layout: &wgpu::BindGroupLayout) -> PanelGeometryFadeImageUniforms {
	// Create the uniforms
	let buffer_descriptor = wgpu::BufferDescriptor {
		label:              Some("zsw-panel-fade-geometry-uniforms-buffer"),
		usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
		size:               u64::try_from(MAX_UNIFORM_SIZE).expect("Maximum uniform size didn't fit into a `u64`"),
		mapped_at_creation: false,
	};
	let buffer = wgpu.device.create_buffer(&buffer_descriptor);

	// Create the uniform bind group
	let bind_group_descriptor = wgpu::BindGroupDescriptor {
		label: Some("zsw-panel-fade-geometry-uniforms-bind-group"),
		layout,
		entries: &[wgpu::BindGroupEntry {
			binding:  0,
			resource: buffer.as_entire_binding(),
		}],
	};
	let bind_group = wgpu.device.create_bind_group(&bind_group_descriptor);

	PanelGeometryFadeImageUniforms { buffer, bind_group }
}

/// Creates the panel slide geometry uniforms
fn create_geometry_slide_uniforms(wgpu: &Wgpu, layout: &wgpu::BindGroupLayout) -> PanelGeometrySlideUniforms {
	// Create the uniforms
	let buffer_descriptor = wgpu::BufferDescriptor {
		label:              Some("zsw-panel-slide-geometry-uniforms-buffer"),
		usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
		size:               u64::try_from(MAX_UNIFORM_SIZE).expect("Maximum uniform size didn't fit into a `u64`"),
		mapped_at_creation: false,
	};
	let buffer = wgpu.device.create_buffer(&buffer_descriptor);

	// Create the uniform bind group
	let bind_group_descriptor = wgpu::BindGroupDescriptor {
		label: Some("zsw-panel-slide-geometry-uniforms-bind-group"),
		layout,
		entries: &[wgpu::BindGroupEntry {
			binding:  0,
			resource: buffer.as_entire_binding(),
		}],
	};
	let bind_group = wgpu.device.create_bind_group(&bind_group_descriptor);

	PanelGeometrySlideUniforms { buffer, bind_group }
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

/// Creates the none uniforms bind group layout
fn create_none_uniforms_bind_group_layout(wgpu: &Wgpu) -> wgpu::BindGroupLayout {
	let descriptor = wgpu::BindGroupLayoutDescriptor {
		label:   Some("zsw-panel-none-geometry-uniforms-bind-group-layout"),
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

	wgpu.device.create_bind_group_layout(&descriptor)
}

/// Creates the fade uniforms bind group layout
fn create_fade_uniforms_bind_group_layout(wgpu: &Wgpu) -> wgpu::BindGroupLayout {
	let descriptor = wgpu::BindGroupLayoutDescriptor {
		label:   Some("zsw-panel-fade-geometry-uniforms-bind-group-layout"),
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

	wgpu.device.create_bind_group_layout(&descriptor)
}

/// Creates the slide uniforms bind group layout
fn create_slide_uniforms_bind_group_layout(wgpu: &Wgpu) -> wgpu::BindGroupLayout {
	let descriptor = wgpu::BindGroupLayoutDescriptor {
		label:   Some("zsw-panel-slide-geometry-uniforms-bind-group-layout"),
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

	wgpu.device.create_bind_group_layout(&descriptor)
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
