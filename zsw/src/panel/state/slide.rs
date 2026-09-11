//! Panel slide state

// TODO: Many functions here need to be de-duplicated with `./fade.rs`.

use {
	crate::{
		panel::{PanelGeometry, PanelSlideShader, geometry, renderer::uniform},
		playlist::PlaylistPlayer,
	},
	app_error::Context,
	chrono::TimeDelta,
	core::{clone::Share, time::Duration},
	euclid::default::{Transform3D, Vector2D},
	image::imageops,
	std::{
		collections::VecDeque,
		path::Path,
		sync::{Arc, OnceLock},
		time::Instant,
	},
	zsw_util::{AppError, Loadable},
	zsw_wgpu::Wgpu,
};

/// Panel slide state
#[derive(Debug)]
pub struct PanelSlideState {
	/// If paused
	paused: bool,

	/// Shader
	shader: PanelSlideShader,

	/// Direction
	// TODO: This should be per-geometry
	dir: PanelSlideDir,

	/// Progress in the first image
	progress: Duration,

	/// Duration of each image
	duration: Duration,

	/// Last time we were updated
	last_update: Instant,

	/// Images
	images: VecDeque<PanelSlideImage>,

	/// Max images
	// TODO: This isn't actually enforced, maybe
	//       we should instead just have a `max_backlog`,
	//       and track how many old images we keep instead.
	max_images: usize,

	/// Image sampler
	image_sampler: OnceLock<wgpu::Sampler>,

	/// Playlist player
	playlist_player: PlaylistPlayer,

	/// Previous image
	prev_image: Loadable<ImageLoadRes>,

	/// Next image
	next_image: Loadable<ImageLoadRes>,
}

impl PanelSlideState {
	/// Creates new state
	pub fn new(
		duration: Duration,
		playlist_player: PlaylistPlayer,
		dir: PanelSlideDir,
		shader: PanelSlideShader,
	) -> Self {
		Self {
			paused: false,
			shader,
			dir,
			progress: Duration::ZERO,
			duration,
			last_update: Instant::now(),
			images: VecDeque::new(),
			// TODO: Adjust this?
			max_images: 3,
			image_sampler: OnceLock::new(),
			playlist_player,
			prev_image: Loadable::new(),
			next_image: Loadable::new(),
		}
	}

	/// Returns the panel shader
	pub fn shader(&self) -> PanelSlideShader {
		self.shader
	}

	/// Returns the direction
	pub fn dir(&self) -> PanelSlideDir {
		self.dir
	}

	/// Returns the image duration
	pub fn duration(&self) -> Duration {
		self.duration
	}

	/// Returns the image progress
	pub fn progress(&self) -> Duration {
		self.progress
	}

	/// Returns all loaded images
	pub fn images(&self) -> impl Iterator<Item = &PanelSlideImage> {
		self.images.iter()
	}

	/// Returns the sampler
	pub fn image_sampler(&self, wgpu: &Arc<Wgpu>) -> &wgpu::Sampler {
		self.image_sampler.get_or_init(|| self::create_image_sampler(wgpu))
	}

	/// Schedules a previous next image.
	fn schedule_load_prev_image(&mut self, wgpu: &Arc<Wgpu>) -> Option<&mut ImageLoadRes> {
		// If we're loaded, just return it
		// Note: We can't use if-let due to a borrow-checker limitation
		if self.prev_image.get().is_some() {
			return self.prev_image.get_mut();
		}

		let (_, path) = self.playlist_player.get(-self.images.len().cast_signed() - 1)?;

		let max_image_size = wgpu.device.limits().max_texture_dimension_2d;

		self.prev_image.try_load(|tx| {
			let wgpu_shared = wgpu.share();
			zsw_util::spawn_task(format!("Load image {path:?}"), move || {
				let image_res = self::load(&wgpu_shared, &path, max_image_size);
				_ = tx.send(ImageLoadRes { path, image_res });

				Ok(())
			});
		})
	}

	/// Schedules a new next image.
	fn schedule_load_next_image(&mut self, wgpu: &Arc<Wgpu>) -> Option<&mut ImageLoadRes> {
		// If we're loaded, just return it
		// Note: We can't use if-let due to a borrow-checker limitation
		if self.next_image.get().is_some() {
			return self.next_image.get_mut();
		}

		let (_, path) = self.playlist_player.get(0)?;

		let max_image_size = wgpu.device.limits().max_texture_dimension_2d;

		self.next_image.try_load(|tx| {
			let wgpu_shared = wgpu.share();
			zsw_util::spawn_task(format!("Load image {path:?}"), move || {
				let image_res = self::load(&wgpu_shared, &path, max_image_size);
				_ = tx.send(ImageLoadRes { path, image_res });

				Ok(())
			});
		})
	}

	/// Loads more images
	pub fn load_next(&mut self, wgpu: &Arc<Wgpu>) {
		_ = self.schedule_load_next_image(wgpu);
	}

	/// Steps this panel's state by a certain number of frames (potentially negative).
	pub fn step(&mut self, wgpu: &Arc<Wgpu>, delta: TimeDelta) {
		let (delta_abs, delta_is_positive) = self::time_delta_to_duration(delta);
		let next_progress = match delta_is_positive {
			true => Some(self.progress.saturating_add(delta_abs)),
			false => self.progress.checked_sub(delta_abs),
		};

		self.progress = match next_progress {
			Some(progress) => progress,
			None => match self.prev_image.take() {
				Some(res) => match res.image_res {
					Ok(image) => {
						self.images.push_front(image);
						self.duration.saturating_sub(delta_abs)
					},
					Err(err) => {
						tracing::warn!("Unable to load image {:?}, removing it from player: {err:?}", res.path);
						_ = self.schedule_load_next_image(wgpu);
						self.playlist_player.remove(&res.path);

						Duration::ZERO
					},
				},
				None => {
					_ = self.schedule_load_prev_image(wgpu);
					Duration::ZERO
				},
			},
		};

		while self.images.len() > self.max_images && self.progress > self.duration {
			_ = self.images.pop_front();
			self.progress = self.progress.saturating_sub(self.duration);
		}

		if let Some(res) = self.next_image.take() {
			self.playlist_player.step_next();
			match res.image_res {
				Ok(image) => self.images.push_back(image),
				Err(err) => {
					tracing::warn!("Unable to load image {:?}, removing it from player: {err:?}", res.path);
					_ = self.schedule_load_next_image(wgpu);
					self.playlist_player.remove(&res.path);
				},
			}
		}
	}

	/// Updates this panel's state using the current time as a delta
	pub fn update(&mut self, wgpu: &Arc<Wgpu>) {
		// Note: We always load images, even if we're paused, since the user might be
		//       moving around manually.
		//self.images.load_missing(&mut self.playlist_player, wgpu);

		// If we're paused, don't update anything
		if self.paused {
			return;
		}

		// Calculate the delta since the last update and step through it.
		// Note: If the delta would be pretty small (sub-millisecond), we
		//       instead skip it.
		// TODO: Revisit this and get the minimum from the lowest refresh rate or something,
		//       since eventually we might want to update at 1000 Hz
		let now = Instant::now();
		let delta = now.duration_since(self.last_update);
		if delta.as_millis() < 1 {
			return;
		}
		self.last_update = now;
		let delta = TimeDelta::from_std(delta).expect("Last update duration didn't fit into a delta");
		self.step(wgpu, delta);
	}

	/// Renders a panel slide's geometry
	pub fn render(
		&mut self,
		shared: &PanelSlideShared,
		wgpu: &Arc<Wgpu>,
		render_pass: &mut wgpu::RenderPass<'_>,
		panel_geometry: &mut PanelGeometry,
		pos_matrix: Transform3D<f32>,
	) {
		let mut missing_images = true;
		let mut cur_global_offset = 0.0;

		let img_offset = self.progress().div_duration_floor(self.duration()) as usize;
		// TODO: Deduplicate this with below
		let local_offset = match self.images().nth(img_offset) {
			Some(image) => {
				let image_size = image.texture_view.texture().size();
				let image_size = Vector2D::new(image_size.width, image_size.height);
				let image_ratio = geometry::image_ratio(panel_geometry.rect, image_size);

				let ratio = match self.dir().is_horizontal() {
					true => image_ratio.y / image_ratio.x,
					false => image_ratio.x / image_ratio.y,
				};

				let offset_abs = self.progress().as_secs_f32() / self.duration().as_secs_f32() - img_offset as f32;
				offset_abs * ratio * 2.0
			},
			None => 0.0,
		};

		for (image_idx, image) in self.images().enumerate().skip(img_offset) {
			// Calculate the position matrix for the panel
			let image_size = image.texture_view.texture().size();
			let image_size = Vector2D::new(image_size.width, image_size.height);
			let image_ratio = geometry::image_ratio(panel_geometry.rect, image_size);

			let offset_abs = cur_global_offset - local_offset;
			if offset_abs > 2.0 {
				missing_images = false;
				break;
			}

			// Bind the geometry uniforms
			let geometry_uniforms = panel_geometry
				.shared
				.slide_or_insert_default()
				.uniforms(wgpu, shared, image_idx);
			render_pass.set_bind_group(0, &geometry_uniforms.bind_group, &[]);

			let ratio = match self.dir().is_horizontal() {
				true => image_ratio.y / image_ratio.x,
				false => image_ratio.x / image_ratio.y,
			};

			// TODO: This should be baked into the position matrix instead.
			let offset: Vector2D<f32> = match self.dir() {
				PanelSlideDir::LeftRight => euclid::vec2(offset_abs, 0.0),
				PanelSlideDir::RightLeft => euclid::vec2(2.0 * (1.0 - ratio) - offset_abs, 0.0),
				PanelSlideDir::UpDown => euclid::vec2(0.0, offset_abs),
				PanelSlideDir::DownUp => euclid::vec2(0.0, 2.0 * (1.0 - ratio) - offset_abs),
			};

			wgpu.write_buffer(&geometry_uniforms.buffer, &uniform::Slide {
				pos_matrix:  uniform::Matrix4x4(pos_matrix.to_arrays()),
				image_ratio: uniform::Vec2(image_ratio.into()),
				offset:      uniform::Vec2(offset.to_array()),
			});

			cur_global_offset += ratio * 2.0;

			let sampler = self.image_sampler(wgpu);
			render_pass.set_bind_group(1, image.bind_group(wgpu, sampler, shared), &[]);

			render_pass.draw_indexed(0..6, 0, 0..1);
		}

		if missing_images {
			self.load_next(wgpu);
		}
	}
}


/// Panel slide geometry shared
#[derive(Default, Debug)]
pub struct PanelSlideGeometryShared {
	/// Uniforms
	pub uniforms: Vec<PanelSlideGeometryUniforms>,
}

impl PanelSlideGeometryShared {
	/// Returns this geometry's uniforms
	pub fn uniforms(
		&mut self,
		wgpu: &Arc<Wgpu>,
		shared: &PanelSlideShared,
		image_idx: usize,
	) -> &mut PanelSlideGeometryUniforms {
		if let Some(uniforms) = self.uniforms.get_mut(image_idx) {
			return uniforms;
		}

		self.uniforms
			.resize_with(image_idx + 1, || self::create_geometry_uniforms(wgpu, shared));
		&mut self.uniforms[image_idx]
	}
}


/// Panel slide shared
#[derive(Debug)]
pub struct PanelSlideShared {
	/// Geometry uniforms bind group layout
	pub geometry_uniforms_bind_group_layout: OnceLock<wgpu::BindGroupLayout>,

	/// Image bind group layout
	pub image_bind_group_layout: OnceLock<wgpu::BindGroupLayout>,
}

impl PanelSlideShared {
	/// Creates the shared
	pub fn new() -> Self {
		Self {
			geometry_uniforms_bind_group_layout: OnceLock::new(),
			image_bind_group_layout:             OnceLock::new(),
		}
	}

	pub fn geometry_uniforms_bind_group_layout(&self, wgpu: &Arc<Wgpu>) -> &wgpu::BindGroupLayout {
		self.geometry_uniforms_bind_group_layout
			.get_or_init(|| self::create_geometry_uniforms_bind_group_layout(wgpu))
	}

	/// Gets the image bind group layout, or initializes it, if uninitialized
	pub fn image_bind_group_layout(&self, wgpu: &Arc<Wgpu>) -> &wgpu::BindGroupLayout {
		self.image_bind_group_layout
			.get_or_init(|| self::create_bind_group_layout(wgpu))
	}
}

/// Panel slide image
#[derive(Debug)]
pub struct PanelSlideImage {
	/// Texture view
	pub texture_view: wgpu::TextureView,

	/// Bind group
	pub bind_group: OnceLock<wgpu::BindGroup>,

	/// Path
	pub _path: Arc<Path>,
}

impl PanelSlideImage {
	/// Gets the bind group, or initializes it, if uninitialized
	pub fn bind_group(&self, wgpu: &Arc<Wgpu>, sampler: &wgpu::Sampler, shared: &PanelSlideShared) -> &wgpu::BindGroup {
		self.bind_group.get_or_init(|| {
			let layout = shared.image_bind_group_layout(wgpu);
			self::create_image_bind_group(wgpu, layout, &self.texture_view, sampler)
		})
	}
}

/// Panel geometry slide uniforms
#[derive(Debug)]
pub struct PanelSlideGeometryUniforms {
	/// Buffer
	pub buffer: wgpu::Buffer,

	/// Bind group
	pub bind_group: wgpu::BindGroup,
}

/// Panel slide direction
#[derive(Clone, Copy, Debug)]
pub enum PanelSlideDir {
	LeftRight,
	RightLeft,
	UpDown,
	DownUp,
}

impl PanelSlideDir {
	/// Returns if this direction is horizontal.
	pub fn is_horizontal(self) -> bool {
		matches!(self, Self::LeftRight | Self::RightLeft)
	}

	/// Returns if this direction is vertical.
	pub fn _is_vertical(self) -> bool {
		matches!(self, Self::UpDown | Self::DownUp)
	}
}

/// Creates the geometry uniforms bind group layout
fn create_geometry_uniforms_bind_group_layout(wgpu: &Arc<Wgpu>) -> wgpu::BindGroupLayout {
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

/// Creates the panel none geometry uniforms
fn create_geometry_uniforms(wgpu: &Arc<Wgpu>, shared: &PanelSlideShared) -> PanelSlideGeometryUniforms {
	// Create the uniforms
	let buffer_descriptor = wgpu::BufferDescriptor {
		label:              Some("zsw-panel-none-geometry-uniforms-buffer"),
		usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
		size:               u64::try_from(
			zsw_util::array_max(&[size_of::<uniform::Slide>()]).expect("No max uniform size"),
		)
		.expect("Maximum uniform size didn't fit into a `u64`"),
		mapped_at_creation: false,
	};
	let buffer = wgpu.device.create_buffer(&buffer_descriptor);

	// Create the uniform bind group
	let bind_group_descriptor = wgpu::BindGroupDescriptor {
		label:   Some("zsw-panel-none-geometry-uniforms-bind-group"),
		layout:  shared.geometry_uniforms_bind_group_layout(wgpu),
		entries: &[wgpu::BindGroupEntry {
			binding:  0,
			resource: buffer.as_entire_binding(),
		}],
	};
	let bind_group = wgpu.device.create_bind_group(&bind_group_descriptor);

	PanelSlideGeometryUniforms { buffer, bind_group }
}

/// Creates the image sampler
fn create_image_sampler(wgpu: &Arc<Wgpu>) -> wgpu::Sampler {
	let descriptor = wgpu::SamplerDescriptor {
		label: Some("zsw-panel-slide-image-sampler"),
		address_mode_u: wgpu::AddressMode::ClampToEdge,
		address_mode_v: wgpu::AddressMode::ClampToEdge,
		address_mode_w: wgpu::AddressMode::ClampToEdge,
		mag_filter: wgpu::FilterMode::Linear,
		min_filter: wgpu::FilterMode::Linear,
		mipmap_filter: wgpu::MipmapFilterMode::Linear,
		..wgpu::SamplerDescriptor::default()
	};
	wgpu.device.create_sampler(&descriptor)
}

/// Creates the image bind group
fn create_image_bind_group(
	wgpu: &Arc<Wgpu>,
	bind_group_layout: &wgpu::BindGroupLayout,
	view: &wgpu::TextureView,
	sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
	let descriptor = wgpu::BindGroupDescriptor {
		label:   Some("zsw-panel-slide-image-bind-group"),
		layout:  bind_group_layout,
		entries: &[
			wgpu::BindGroupEntry {
				binding:  0,
				resource: wgpu::BindingResource::TextureView(view),
			},
			wgpu::BindGroupEntry {
				binding:  1,
				resource: wgpu::BindingResource::Sampler(sampler),
			},
		],
	};
	wgpu.device.create_bind_group(&descriptor)
}

/// Creates the slide image bind group layout
fn create_bind_group_layout(wgpu: &Arc<Wgpu>) -> wgpu::BindGroupLayout {
	let descriptor = wgpu::BindGroupLayoutDescriptor {
		label:   Some("zsw-panel-slide-image-bind-group-layout"),
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

	wgpu.device.create_bind_group_layout(&descriptor)
}

#[derive(Debug)]
pub struct ImageLoadRes {
	path:      Arc<Path>,
	image_res: Result<PanelSlideImage, AppError>,
}

/// Loads an image
pub fn load(wgpu: &Wgpu, path: &Arc<Path>, max_image_size: u32) -> Result<PanelSlideImage, AppError> {
	// Load the image
	tracing::trace!("Loading image {:?}", path);
	let mut image = image::open(path).context("Unable to open image")?;
	tracing::trace!("Loaded image {:?} ({}x{})", path, image.width(), image.height());

	// If the image is too big, resize it
	if image.width() >= max_image_size || image.height() >= max_image_size {
		tracing::trace!(
			"Resizing image {:?} ({}x{}) to at most {max_image_size}x{max_image_size}",
			path,
			image.width(),
			image.height()
		);
		image = image.resize(max_image_size, max_image_size, imageops::FilterType::Nearest);
		tracing::trace!("Resized image {:?} to {}x{}", path, image.width(), image.height());
	}

	let texture_label = format!("zsw-panel-slide-image-texture[path={path:?}]");
	let (_texture, texture_view) = wgpu
		.create_texture_from_image(&texture_label, image)
		.context("Unable to create texture for image")?;

	let image = PanelSlideImage {
		texture_view,
		bind_group: OnceLock::new(),
		_path: path.share(),
	};

	Ok(image)
}

/// Converts a chrono time delta into a duration, indicating whether it's positive or negative
fn time_delta_to_duration(delta: TimeDelta) -> (Duration, bool) {
	match delta.to_std() {
		Ok(delta) => (delta, true),
		Err(_) => ((-delta).to_std().expect("Duration should fit"), false),
	}
}
