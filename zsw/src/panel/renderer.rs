//! Panels renderer

// Modules
mod uniform;
mod vertex;

// Exports
pub use self::{uniform::MAX_UNIFORM_SIZE, vertex::PanelVertex};

// Imports
use {
	self::uniform::PanelImageUniforms,
	super::{Panel, PanelImage, PanelImages, PanelName, Panels},
	crate::{config_dirs::ConfigDirs, panel::PanelGeometry},
	cgmath::Vector2,
	core::{
		future::Future,
		mem::{self, Discriminant},
	},
	itertools::Itertools,
	naga_oil::compose::{ComposableModuleDescriptor, Composer, ImportDefinition, NagaModuleDescriptor, ShaderDefValue},
	std::{
		borrow::Cow,
		collections::{HashMap, HashSet, hash_map},
		path::Path,
	},
	tokio::fs,
	wgpu::{naga, util::DeviceExt},
	winit::dpi::PhysicalSize,
	zsw_util::Rect,
	zsw_wgpu::{FrameRender, WgpuRenderer, WgpuShared},
	app_error::{AppError, Context},
};

/// Panels renderer layouts
#[derive(Debug)]
pub struct PanelsRendererLayouts {
	/// Uniforms bind group layout
	pub uniforms_bind_group_layout: wgpu::BindGroupLayout,

	/// Image bind group layout
	pub image_bind_group_layout: wgpu::BindGroupLayout,
}

impl PanelsRendererLayouts {
	/// Creates new layouts for the panels renderer
	pub fn new(wgpu_shared: &WgpuShared) -> Self {
		let uniforms_bind_group_layout = self::create_uniforms_bind_group_layout(wgpu_shared);
		let image_bind_group_layout = self::create_image_bind_group_layout(wgpu_shared);

		Self {
			uniforms_bind_group_layout,
			image_bind_group_layout,
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
	/// Render pipeline for each shader
	// TODO: Prune ones that aren't used?
	// TODO: Using `mem::discriminant` here could lead to some subtle bugs
	//       where we expect a certain shader to update based on it's fields,
	//       so we should probably convert this into something else that uniquely
	//       determines whether a render pipeline update is required or not.
	render_pipelines: HashMap<Discriminant<PanelShader>, wgpu::RenderPipeline>,

	/// Vertex buffer
	vertices: wgpu::Buffer,

	/// Index buffer
	indices: wgpu::Buffer,

	/// Msaa frame-buffer
	msaa_framebuffer: wgpu::TextureView,
}

impl PanelsRenderer {
	/// Creates a new renderer for the panels
	pub async fn new(wgpu_renderer: &WgpuRenderer, wgpu_shared: &WgpuShared) -> Result<Self, AppError> {
		// Create the index / vertex buffer
		let indices = self::create_indices(wgpu_shared);
		let vertices = self::create_vertices(wgpu_shared);

		// Create the framebuffer
		let msaa_framebuffer = self::create_msaa_framebuffer(wgpu_renderer, wgpu_shared, wgpu_renderer.surface_size());

		Ok(Self {
			render_pipelines: HashMap::new(),
			vertices,
			indices,
			msaa_framebuffer,
		})
	}

	/// Resizes the buffer
	pub fn resize(&mut self, wgpu_renderer: &WgpuRenderer, wgpu_shared: &WgpuShared, size: PhysicalSize<u32>) {
		tracing::debug!("Resizing msaa framebuffer to {}x{}", size.width, size.height);
		self.msaa_framebuffer = self::create_msaa_framebuffer(wgpu_renderer, wgpu_shared, size);
	}

	/// Renders a panel
	#[expect(clippy::too_many_arguments, reason = "TODO: Remove something")]
	pub async fn render(
		&mut self,
		frame: &mut FrameRender,
		config_dirs: &ConfigDirs,
		wgpu_renderer: &WgpuRenderer,
		wgpu_shared: &WgpuShared,
		layouts: &PanelsRendererLayouts,
		geometry_uniforms: &mut PanelsGeometryUniforms,
		window_geometry: &Rect<i32, u32>,
		panels: &Panels,
		panels_images: &HashMap<PanelName, PanelImages>,
	) -> Result<(), AppError> {
		// Create the render pass for all panels
		let render_pass_color_attachment = match MSAA_SAMPLES {
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
			label:                    Some("[zsw::panel_renderer] Render pass"),
			color_attachments:        &[Some(render_pass_color_attachment)],
			depth_stencil_attachment: None,
			timestamp_writes:         None,
			occlusion_query_set:      None,
		};
		let mut render_pass = frame.encoder.begin_render_pass(&render_pass_descriptor);

		// Set our shared indices and vertices
		render_pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
		render_pass.set_vertex_buffer(0, self.vertices.slice(..));

		for panel in panels.get_all().await {
			let panel = panel.lock().await;

			// If the panel images are missing or empty, skip this panel
			let Some(panel_images) = panels_images.get(&panel.name) else {
				continue;
			};
			if panel_images.is_empty() {
				continue;
			}

			let render_pipeline = match self.render_pipelines.entry(mem::discriminant(&panel.shader)) {
				hash_map::Entry::Occupied(entry) => entry.into_mut(),
				hash_map::Entry::Vacant(entry) => {
					let render_pipeline = self::create_render_pipeline(
						config_dirs,
						wgpu_renderer,
						wgpu_shared,
						&layouts.uniforms_bind_group_layout,
						&layouts.image_bind_group_layout,
						panel.shader,
					)
					.await
					.context("Unable to create render pipeline")?;

					entry.insert(render_pipeline)
				},
			};

			// Bind the pipeline for the specific shader
			render_pass.set_pipeline(render_pipeline);

			// Bind the panel-shared image bind group
			render_pass.set_bind_group(1, &panel_images.image_bind_group, &[]);

			for geometry in &panel.geometries {
				// If this geometry is outside our window, we can safely ignore it
				if window_geometry.intersection(geometry.geometry).is_none() {
					continue;
				}

				let geometry_uniforms = geometry_uniforms.get(&panel.name, &geometry.geometry, wgpu_shared, layouts);

				// Write the uniforms
				Self::write_uniforms(
					wgpu_shared,
					frame.surface_size,
					&panel,
					panel_images,
					window_geometry,
					geometry,
					&geometry_uniforms.buffer,
				);

				// Then bind the geometry uniforms and draw
				render_pass.set_bind_group(0, &geometry_uniforms.bind_group, &[]);
				render_pass.draw_indexed(0..6, 0, 0..1);
			}
		}

		Ok(())
	}

	/// Writes the uniforms
	pub fn write_uniforms(
		wgpu_shared: &WgpuShared,
		surface_size: PhysicalSize<u32>,
		panel: &Panel,
		panel_images: &PanelImages,
		window_geometry: &Rect<i32, u32>,
		geometry: &PanelGeometry,
		geometry_uniforms: &wgpu::Buffer,
	) {
		// Calculate the position matrix for the panel
		let pos_matrix = geometry.pos_matrix(window_geometry, surface_size);
		let pos_matrix = uniform::Matrix4x4(pos_matrix.into());

		let image_uniforms = |image: &PanelImage| {
			let (size, swap_dir) = match *image {
				PanelImage::Empty => (Vector2::new(0, 0), false),
				PanelImage::Loaded { size, swap_dir, .. } => (size, swap_dir),
			};

			let ratio = PanelGeometry::image_ratio(geometry.geometry.size, size);
			PanelImageUniforms::new(ratio, swap_dir)
		};

		let prev = image_uniforms(&panel_images.prev);
		let cur = image_uniforms(&panel_images.cur);
		let next = image_uniforms(&panel_images.next);

		// Writes uniforms `uniforms`
		let write_uniforms = |uniforms_bytes| wgpu_shared.queue.write_buffer(geometry_uniforms, 0, uniforms_bytes);
		macro write_uniforms($uniforms:expr) {
			write_uniforms(bytemuck::bytes_of(&$uniforms))
		}

		let fade_point = panel.state.fade_point_norm();
		let progress = panel.state.progress_norm();
		match panel.shader {
			PanelShader::None { background_color } => write_uniforms!(uniform::None {
				pos_matrix,
				background_color: uniform::Vec4(background_color),
			}),
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
		}
	}
}

/// Panels geometry uniforms
// TODO: Instead of using `Rect<i32, u32>` as the key, refer to it by index,
//       or give geometries names like panels do.
#[derive(Debug)]
pub struct PanelsGeometryUniforms(HashMap<(PanelName, Rect<i32, u32>), PanelGeometryUniforms>);

impl PanelsGeometryUniforms {
	/// Creates an empty set of geometry uniforms
	pub fn new() -> Self {
		Self(HashMap::new())
	}

	// Gets, or creates the uniforms for a geometry
	pub fn get(
		&mut self,
		panel: &PanelName,
		geometry: &Rect<i32, u32>,
		wgpu_shared: &WgpuShared,
		layouts: &PanelsRendererLayouts,
	) -> &PanelGeometryUniforms {
		self.0
			.entry((panel.clone(), *geometry))
			.or_insert_with(|| PanelGeometryUniforms::new(wgpu_shared, layouts))
	}
}

/// Panel geometry uniforms
#[derive(Debug)]
pub struct PanelGeometryUniforms {
	/// Buffer
	pub buffer: wgpu::Buffer,

	/// Bind group
	pub bind_group: wgpu::BindGroup,
}

impl PanelGeometryUniforms {
	/// Creates the panel geometry uniforms
	fn new(wgpu_shared: &WgpuShared, layouts: &PanelsRendererLayouts) -> Self {
		// Create the uniforms
		let buffer_descriptor = wgpu::BufferDescriptor {
			label:              None,
			usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
			size:               u64::try_from(MAX_UNIFORM_SIZE).expect("Maximum uniform size didn't fit into a `u64`"),
			mapped_at_creation: false,
		};
		let buffer = wgpu_shared.device.create_buffer(&buffer_descriptor);

		// Create the uniform bind group
		let bind_group_descriptor = wgpu::BindGroupDescriptor {
			layout:  &layouts.uniforms_bind_group_layout,
			entries: &[wgpu::BindGroupEntry {
				binding:  0,
				resource: buffer.as_entire_binding(),
			}],
			label:   None,
		};
		let bind_group = wgpu_shared.device.create_bind_group(&bind_group_descriptor);

		Self { buffer, bind_group }
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
async fn create_render_pipeline(
	config_dirs: &ConfigDirs,
	wgpu_renderer: &WgpuRenderer,
	wgpu_shared: &WgpuShared,
	uniforms_bind_group_layout: &wgpu::BindGroupLayout,
	image_bind_group_layout: &wgpu::BindGroupLayout,
	shader: PanelShader,
) -> Result<wgpu::RenderPipeline, AppError> {
	let shader_path = config_dirs.shaders().join(shader.path());

	tracing::debug!("Creating render pipeline for shader {shader:?} from {shader_path:?}");

	let shader_module = self::parse_shader(&shader_path, shader)
		.await
		.with_context(|| format!("Unable to preprocess shader {shader_path:?}"))?;

	// Load the shader
	let shader_descriptor = wgpu::ShaderModuleDescriptor {
		label:  Some("[zsw::panel_renderer] Shader"),
		source: wgpu::ShaderSource::Naga(Cow::Owned(shader_module)),
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
			count: MSAA_SAMPLES,
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

	Ok(wgpu_shared.device.create_render_pipeline(&render_pipeline_descriptor))
}

/// Parses the shader
async fn parse_shader(shader_path: &Path, shader: PanelShader) -> Result<naga::Module, AppError> {
	// Read the initial shader
	let shader_dir = shader_path.parent().context("Shader path had no parent directory")?;
	let shader_path = shader_path.as_os_str().to_str().context("Shader path must be UTF-8")?;
	let shader_source = fs::read_to_string(shader_path)
		.await
		.context("Unable to read shader file")?;

	// Import all modules that we need, starting with the main file and recursively
	// getting them all
	let mut composer = Composer::default();
	let (_, required_modules, _) = naga_oil::compose::get_preprocessor_data(&shader_source);
	for module in required_modules {
		self::parse_shader_module(shader_dir, &mut composer, &module)
			.await
			.with_context(|| format!("Unable to build import {:?}", module.import))?;
	}

	let mut shader_defs = HashSet::new();
	#[expect(unused_results, reason = "We don't care about whether it exists or not")]
	match shader {
		PanelShader::None { .. } => {},
		PanelShader::Fade => {
			shader_defs.insert("FADE_BASIC");
		},
		PanelShader::FadeWhite { .. } => {
			shader_defs.insert("FADE_WHITE");
		},
		PanelShader::FadeOut { .. } => {
			shader_defs.insert("FADE_OUT");
		},
		PanelShader::FadeIn { .. } => {
			shader_defs.insert("FADE_IN");
		},
	}

	// And finally build the final module.
	let shader_module = composer
		.make_naga_module(NagaModuleDescriptor {
			source: &shader_source,
			file_path: shader_path,
			shader_type: naga_oil::compose::ShaderType::Wgsl,
			shader_defs: shader_defs
				.into_iter()
				.map(|def| (def.to_owned(), ShaderDefValue::Bool(true)))
				.collect(),
			..Default::default()
		})
		.context("Unable to make naga module")?;

	Ok(shader_module)
}

/// Parses a shader module
#[expect(clippy::manual_async_fn, reason = "We miss out on the `+ Send` bound if we do.")]
fn parse_shader_module(
	shader_dir: &Path,
	composer: &mut Composer,
	module: &ImportDefinition,
) -> impl Future<Output = Result<(), AppError>> + Send {
	async move {
		// If we already have the module, continue
		if composer.contains_module(&module.import) {
			return Ok(());
		}

		// Else read the module
		let module_path_rel = module.import.split("::").join("/");
		let module_path = shader_dir.join(&module_path_rel).with_extension("wgsl");
		let module_path = module_path.to_str().context("Module file name was non-utf8")?;
		let module_source = fs::read_to_string(module_path)
			.await
			.context("Unable to read module file")?;

		// And get all required imports
		let (_, required_modules, _) = naga_oil::compose::get_preprocessor_data(&module_source);
		for module in required_modules {
			Box::pin(self::parse_shader_module(shader_dir, composer, &module))
				.await
				.with_context(|| format!("Unable to build import {:?}", module.import))?;
		}

		// Then add it as a module
		tracing::trace!("Processing shader {shader_dir:?} module {module_path:?}");
		_ = composer
			.add_composable_module(ComposableModuleDescriptor {
				source: &module_source,
				file_path: module_path,
				language: naga_oil::compose::ShaderLanguage::Wgsl,
				as_name: Some(module.import.clone()),
				..Default::default()
			})
			.context("Unable to parse module")?;

		Ok(())
	}
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
	let msaa_frame_descriptor = wgpu::TextureDescriptor {
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
		.create_texture(&msaa_frame_descriptor)
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
#[expect(variant_size_differences, reason = "16 bytes is still reasonable for this type")]
pub enum PanelShader {
	None { background_color: [f32; 4] },
	Fade,
	FadeWhite { strength: f32 },
	FadeOut { strength: f32 },
	FadeIn { strength: f32 },
}
impl PanelShader {
	/// Returns this shader's path, relative to the shaders path
	pub fn path(self) -> &'static str {
		match self {
			Self::None { .. } => "panels/none.wgsl",
			Self::Fade | Self::FadeWhite { .. } | Self::FadeOut { .. } | Self::FadeIn { .. } => "panels/fade.wgsl",
		}
	}

	/// Returns this shader's name
	pub fn name(self) -> &'static str {
		match self {
			Self::None { .. } => "None",
			Self::Fade => "Fade",
			Self::FadeWhite { .. } => "Fade white",
			Self::FadeOut { .. } => "Fade out",
			Self::FadeIn { .. } => "Fade in",
		}
	}
}
