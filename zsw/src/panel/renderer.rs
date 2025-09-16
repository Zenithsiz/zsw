//! Panels renderer

// Modules
mod uniform;
mod vertex;

// Exports
pub use self::{uniform::MAX_UNIFORM_SIZE, vertex::PanelVertex};

// Imports
use {
	self::uniform::PanelImageUniforms,
	super::{PanelGeometryUniforms, PanelState, Panels, state::fade::PanelFadeImagesShared},
	crate::{
		metrics::{self, Metrics},
		panel::{PanelGeometry, state::fade::PanelFadeImage},
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
	/// Render pipeline for each shader by shader name
	// TODO: Prune ones that aren't used?
	render_pipelines: Mutex<HashMap<&'static str, Arc<wgpu::RenderPipeline>>>,

	/// Vertex buffer
	vertices: wgpu::Buffer,

	/// Index buffer
	indices: wgpu::Buffer,

	/// Uniforms bind group layout
	uniforms_bind_group_layout: wgpu::BindGroupLayout,

	/// Fade
	fade: PanelFadeImagesShared,
}

impl PanelsRendererShared {
	/// Creates new layouts for the panels renderer
	pub fn new(wgpu: &Wgpu) -> Self {
		// Create the index / vertex buffer
		let indices = self::create_indices(wgpu);
		let vertices = self::create_vertices(wgpu);

		let uniforms_bind_group_layout = self::create_uniforms_bind_group_layout(wgpu);

		Self {
			render_pipelines: Mutex::new(HashMap::new()),
			vertices,
			indices,
			uniforms_bind_group_layout,
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
			label:                    Some("[zsw::panel] Render pass"),
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
		let mut panel_metrics = HashMap::new();
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

			#[time(create_render_pipeline)]
			let render_pipeline = match shared.render_pipelines.lock().await.entry(panel.state.shader().name()) {
				hash_map::Entry::Occupied(entry) => Arc::clone(entry.get()),
				hash_map::Entry::Vacant(entry) => {
					let bind_group_layouts = match panel.state {
						PanelState::None(_) => &[&shared.uniforms_bind_group_layout] as &[_],
						PanelState::Fade(_) => &[
							&shared.uniforms_bind_group_layout,
							shared.fade.image_bind_group_layout(wgpu).await,
						],
						PanelState::Slide(_) => &[&shared.uniforms_bind_group_layout],
					};

					let render_pipeline = self::create_render_pipeline(
						wgpu_renderer,
						wgpu,
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

			// Bind the extra bind groups
			match &mut panel.state {
				PanelState::None(_panel_state) => (),
				PanelState::Fade(panel_state) =>
					render_pass.set_bind_group(1, panel_state.images().image_bind_group(wgpu, &shared.fade).await, &[]),
				PanelState::Slide(_panel_state) => (),
			}

			// The display might have changed asynchronously from the panel geometries,
			// so resize it to ensure we have a panel geometry for each display geometry.
			let display = panel.display.read().await;
			panel
				.geometries
				.resize_with(display.geometries.len(), PanelGeometry::new);

			let geometry_metrics = panel_metrics
				.entry(panel_idx)
				.or_insert_with(|| metrics::RenderPanelFrameTime {
					update_panel,
					create_render_pipeline,
					geometries: HashMap::new(),
				});
			for (geometry_idx, (&display_geometry, panel_geometry)) in
				display.geometries.iter().zip_eq(&mut panel.geometries).enumerate()
			{
				// If this geometry is outside our window, we can safely ignore it
				if window_geometry.intersection(display_geometry).is_none() {
					continue;
				}

				// Write and bind the uniforms
				#[time(write_uniforms)]
				Self::write_bind_uniforms(
					wgpu,
					shared,
					frame.surface_size,
					&panel.state,
					window_geometry,
					window,
					display_geometry,
					panel_geometry,
					&mut render_pass,
				);

				// Finally draw
				#[time(draw)]
				render_pass.draw_indexed(0..6, 0, 0..1);

				_ = geometry_metrics
					.geometries
					.insert(geometry_idx, metrics::RenderPanelGeometryFrameTime {
						write_uniforms,
						draw,
					});
			}
		}

		metrics
			.render_panels_frame_times(window.id())
			.await
			.add(metrics::RenderPanelsFrameTime {
				create_render_pass,
				lock_panels,
				panels: panel_metrics,
			});

		Ok(())
	}

	/// Writes and binds the uniforms
	pub fn write_bind_uniforms(
		wgpu: &Wgpu,
		shared: &PanelsRendererShared,
		surface_size: PhysicalSize<u32>,
		panel_state: &PanelState,
		window_geometry: Rect<i32, u32>,
		window: &Window,
		display_geometry: Rect<i32, u32>,
		panel_geometry: &mut PanelGeometry,
		render_pass: &mut wgpu::RenderPass<'_>,
	) {
		// Calculate the position matrix for the panel
		let pos_matrix = PanelGeometry::pos_matrix(display_geometry, window_geometry, surface_size);
		let pos_matrix = uniform::Matrix4x4(pos_matrix.into());

		// Writes uniforms `uniforms`
		let geometry_uniforms = panel_geometry
			.uniforms
			.entry(window.id())
			.or_insert_with(|| self::create_panel_geometry_uniforms(wgpu, shared));
		let write_uniforms = |uniforms_bytes| {
			wgpu.queue.write_buffer(&geometry_uniforms.buffer, 0, uniforms_bytes);
		};
		macro write_uniforms($uniforms:expr) {
			write_uniforms(bytemuck::bytes_of(&$uniforms))
		}

		match panel_state {
			PanelState::None(panel_state) => write_uniforms!(uniform::None {
				pos_matrix,
				background_color: uniform::Vec4(panel_state.background_color),
			}),
			PanelState::Fade(panel_state) => {
				let image_uniforms = |image: Option<&PanelFadeImage>| {
					let (size, swap_dir) = match image {
						None => (Vector2::new(0, 0), false),
						Some(image) => {
							let texture = image.texture_view.texture();
							let size = Vector2::new(texture.width(), texture.height());
							(size, image.swap_dir)
						},
					};

					let ratio = PanelGeometry::image_ratio(display_geometry.size, size);
					PanelImageUniforms::new(ratio, swap_dir)
				};

				let prev = image_uniforms(panel_state.images().prev.as_ref());
				let cur = image_uniforms(panel_state.images().cur.as_ref());
				let next = image_uniforms(panel_state.images().next.as_ref());

				let fade_duration = panel_state.fade_duration_norm();
				let progress = panel_state.progress_norm();
				match panel_state.shader() {
					PanelFadeShader::Basic => write_uniforms!(uniform::Fade {
						pos_matrix,
						prev,
						cur,
						next,
						fade_duration,
						progress,
						_unused: [0; 2],
					}),
					PanelFadeShader::White { strength } => write_uniforms!(uniform::FadeWhite {
						pos_matrix,
						prev,
						cur,
						next,
						fade_duration,
						progress,
						strength,
						_unused: 0,
					}),
					PanelFadeShader::Out { strength } => write_uniforms!(uniform::FadeOut {
						pos_matrix,
						prev,
						cur,
						next,
						fade_duration,
						progress,
						strength,
						_unused: 0,
					}),
					PanelFadeShader::In { strength } => write_uniforms!(uniform::FadeIn {
						pos_matrix,
						prev,
						cur,
						next,
						fade_duration,
						progress,
						strength,
						_unused: 0,
					}),
				}
			},
			PanelState::Slide(_panel_state) => write_uniforms!(uniform::Slide { pos_matrix }),
		}

		render_pass.set_bind_group(0, &geometry_uniforms.bind_group, &[]);
	}
}

/// Creates the panel geometry uniforms
fn create_panel_geometry_uniforms(wgpu: &Wgpu, shared: &PanelsRendererShared) -> PanelGeometryUniforms {
	// Create the uniforms
	let buffer_descriptor = wgpu::BufferDescriptor {
		label:              Some("[zsw::panel] Geometry uniforms buffer"),
		usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
		size:               u64::try_from(MAX_UNIFORM_SIZE).expect("Maximum uniform size didn't fit into a `u64`"),
		mapped_at_creation: false,
	};
	let buffer = wgpu.device.create_buffer(&buffer_descriptor);

	// Create the uniform bind group
	let bind_group_descriptor = wgpu::BindGroupDescriptor {
		layout:  &shared.uniforms_bind_group_layout,
		entries: &[wgpu::BindGroupEntry {
			binding:  0,
			resource: buffer.as_entire_binding(),
		}],
		label:   Some("[zsw::panel] Geometry uniforms bind group"),
	};
	let bind_group = wgpu.device.create_bind_group(&bind_group_descriptor);

	PanelGeometryUniforms { buffer, bind_group }
}

/// Creates the vertices
fn create_vertices(wgpu: &Wgpu) -> wgpu::Buffer {
	let descriptor = wgpu::util::BufferInitDescriptor {
		label:    Some("[zsw::panel] Vertex buffer"),
		contents: bytemuck::cast_slice(&PanelVertex::QUAD),
		usage:    wgpu::BufferUsages::VERTEX,
	};

	wgpu.device.create_buffer_init(&descriptor)
}

/// Creates the indices
fn create_indices(wgpu: &Wgpu) -> wgpu::Buffer {
	const INDICES: [u32; 6] = [0, 1, 3, 0, 3, 2];
	let descriptor = wgpu::util::BufferInitDescriptor {
		label:    Some("[zsw::panel] Index buffer"),
		contents: bytemuck::cast_slice(&INDICES),
		usage:    wgpu::BufferUsages::INDEX,
	};

	wgpu.device.create_buffer_init(&descriptor)
}

/// Creates the render pipeline
fn create_render_pipeline(
	wgpu_renderer: &WgpuRenderer,
	wgpu: &Wgpu,
	bind_group_layouts: &[&wgpu::BindGroupLayout],
	shader: PanelShader,
	msaa_samples: u32,
) -> Result<wgpu::RenderPipeline, AppError> {
	let shader_name = shader.name();
	tracing::debug!("Creating render pipeline for shader {shader_name:?}");

	// Parse the shader from the build script
	let shader_module =
		serde_json::from_str::<naga::Module>(shader.module_json()).context("Serialized shader module was invalid")?;

	// Load the shader
	let shader_descriptor = wgpu::ShaderModuleDescriptor {
		label:  Some(&format!("[zsw::panel] Shader {shader_name:?}")),
		source: wgpu::ShaderSource::Naga(Cow::Owned(shader_module)),
	};
	let shader = wgpu.device.create_shader_module(shader_descriptor);

	// Create the pipeline layout
	let render_pipeline_layout_descriptor = wgpu::PipelineLayoutDescriptor {
		label: Some(&format!("[zsw::panel] Shader {shader_name:?} render pipeline layout")),
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
		label:  Some(&format!("[zsw::panel] Shader {shader_name:?} render pipeline")),
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
		size:            msaa_texture_extent,
		mip_level_count: 1,
		sample_count:    msaa_samples,
		dimension:       wgpu::TextureDimension::D2,
		format:          surface_config.format,
		usage:           wgpu::TextureUsages::RENDER_ATTACHMENT,
		label:           Some("[zsw::panel] MSAA framebuffer"),
		view_formats:    &surface_config.view_formats,
	};

	wgpu.device
		.create_texture(&msaa_frame_descriptor)
		.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Creates the uniforms bind group layout
fn create_uniforms_bind_group_layout(wgpu: &Wgpu) -> wgpu::BindGroupLayout {
	let descriptor = wgpu::BindGroupLayoutDescriptor {
		label:   Some("[zsw::panel] Geometry uniforms bind group layout"),
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
