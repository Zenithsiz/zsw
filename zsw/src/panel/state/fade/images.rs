//! Panel fade images

// Imports
use {
	crate::{panel::renderer::uniform, playlist::PlaylistPlayer},
	app_error::Context,
	image::{DynamicImage, imageops},
	std::{self, collections::HashMap, mem, path::Path, sync::Arc},
	tokio::sync::{Mutex, OnceCell},
	winit::window::WindowId,
	zsw_util::{AppError, Loadable},
	zsw_wgpu::Wgpu,
	zutil_cloned::cloned,
};

/// Panel fade images shared
#[derive(Debug)]
pub struct PanelFadeImagesShared {
	/// Geometry uniforms bind group layout
	pub geometry_uniforms_bind_group_layout: wgpu::BindGroupLayout,

	/// Image bind group layout
	pub image_bind_group_layout: OnceCell<wgpu::BindGroupLayout>,
}

impl PanelFadeImagesShared {
	/// Creates the shared
	pub fn new(wgpu: &Wgpu) -> Self {
		let geometry_uniforms_bind_group_layout = self::create_geometry_uniforms_bind_group_layout(wgpu);

		Self {
			geometry_uniforms_bind_group_layout,
			image_bind_group_layout: OnceCell::new(),
		}
	}

	/// Gets the image bind group layout, or initializes it, if uninitialized
	pub async fn image_bind_group_layout(&self, wgpu: &Wgpu) -> &wgpu::BindGroupLayout {
		self.image_bind_group_layout
			.get_or_init(async || self::create_bind_group_layout(wgpu))
			.await
	}
}

/// Panel fade images
#[derive(Debug)]
pub struct PanelFadeImages {
	/// Previous image
	pub prev: Option<PanelFadeImage>,

	/// Current image
	pub cur: Option<PanelFadeImage>,

	/// Next image
	pub next: Option<PanelFadeImage>,

	/// Image sampler
	pub image_sampler: OnceCell<wgpu::Sampler>,

	/// Next image
	pub next_image: Loadable<ImageLoadRes>,
}

/// Panel's fade image
#[derive(Debug)]
pub struct PanelFadeImage {
	/// Texture view
	pub texture_view: wgpu::TextureView,

	/// Bind group
	pub bind_group: OnceCell<wgpu::BindGroup>,

	/// Geometry uniforms
	pub geometry_uniforms: Mutex<HashMap<(WindowId, usize), Arc<PanelFadeImageGeometryUniforms>>>,

	/// Swap direction
	pub swap_dir: bool,

	/// Path
	pub path: Arc<Path>,
}

impl PanelFadeImage {
	/// Gets the bind group, or initializes it, if uninitialized
	pub async fn bind_group(
		&self,
		wgpu: &Wgpu,
		sampler: &wgpu::Sampler,
		shared: &PanelFadeImagesShared,
	) -> &wgpu::BindGroup {
		self.bind_group
			.get_or_init(async || {
				let layout = shared.image_bind_group_layout(wgpu).await;
				self::create_image_bind_group(wgpu, layout, &self.texture_view, sampler)
			})
			.await
	}

	/// Returns the geometry uniforms
	pub async fn geometry_uniforms(
		&self,
		wgpu: &Wgpu,
		shared: &PanelFadeImagesShared,
		window_id: WindowId,
		geometry_idx: usize,
	) -> Arc<PanelFadeImageGeometryUniforms> {
		let mut geometry_uniforms = self.geometry_uniforms.lock().await;
		let geometry_uniforms = geometry_uniforms
			.entry((window_id, geometry_idx))
			.or_insert_with(|| Arc::new(self::create_image_geometry_uniforms(wgpu, shared)));
		Arc::clone(geometry_uniforms)
	}
}

impl PanelFadeImages {
	/// Creates a new panel
	#[must_use]
	pub fn new() -> Self {
		Self {
			prev:          None,
			cur:           None,
			next:          None,
			image_sampler: OnceCell::new(),
			next_image:    Loadable::new(),
		}
	}

	/// Returns an iterator over all images
	pub fn iter(&self) -> impl Iterator<Item = (PanelFadeImageSlot, &PanelFadeImage)> {
		[
			(PanelFadeImageSlot::Prev, &self.prev),
			(PanelFadeImageSlot::Cur, &self.cur),
			(PanelFadeImageSlot::Next, &self.next),
		]
		.into_iter()
		.filter_map(|(slot, img)| img.as_ref().map(|img| (slot, img)))
	}

	/// Steps to the previous image, if any
	///
	/// If successful, starts loading any missing images
	///
	/// Returns `Err(())` if this would erase the current image.
	pub fn step_prev(&mut self, playlist_player: &mut PlaylistPlayer, wgpu: &Wgpu) -> Result<(), ()> {
		playlist_player.step_prev()?;
		mem::swap(&mut self.cur, &mut self.next);
		mem::swap(&mut self.prev, &mut self.cur);
		self.prev = None;
		self.load_missing(playlist_player, wgpu);

		Ok(())
	}

	/// Steps to the next image.
	///
	/// If successful, starts loading any missing images
	///
	/// Returns `Err(())` if this would erase the current image.
	pub fn step_next(&mut self, playlist_player: &mut PlaylistPlayer, wgpu: &Wgpu) -> Result<(), ()> {
		if self.next.is_none() {
			return Err(());
		}

		playlist_player.step_next();
		mem::swap(&mut self.prev, &mut self.cur);
		mem::swap(&mut self.cur, &mut self.next);
		self.next = None;
		self.load_missing(playlist_player, wgpu);

		Ok(())
	}

	/// Gets the image sampler, or initializes it, if uninitialized
	pub async fn image_sampler(&self, wgpu: &Wgpu) -> &wgpu::Sampler {
		self.image_sampler
			.get_or_init(async || self::create_image_sampler(wgpu))
			.await
	}

	/// Loads any missing images, prioritizing the current, then next, then previous.
	///
	/// Requests images if missing any.
	pub fn load_missing(&mut self, playlist_player: &mut PlaylistPlayer, wgpu: &Wgpu) {
		// Get the next image, if we can
		let Some(res) = self.next_image(playlist_player, wgpu) else {
			return;
		};

		// Then check if we got the image
		let image = match res.image_res {
			// If so, return it
			Ok(image) => image,

			// Else, log an error, remove the image and re-schedule it
			Err(err) => {
				tracing::warn!(
					"Unable to load image {:?}, removing it from player: {}",
					res.path,
					err.pretty()
				);
				playlist_player.remove(&res.path);

				_ = self.schedule_load_image(playlist_player, wgpu);
				return;
			},
		};

		// Get which slot to load the image into
		let slot = {
			match res.playlist_pos {
				pos if Some(pos) == playlist_player.prev_pos() => Some(PanelFadeImageSlot::Prev),
				pos if pos == playlist_player.cur_pos() => Some(PanelFadeImageSlot::Cur),
				pos if pos == playlist_player.next_pos() => Some(PanelFadeImageSlot::Next),
				pos => {
					tracing::warn!(
						pos,
						playlist_pos = playlist_player.cur_pos(),
						"Discarding loaded image due to position being too far",
					);
					None
				},
			}
		};

		if let Some(slot) = slot {
			let texture_label = format!("zsw-panel-fade-image-texture[path={:?}]", res.path);
			let texture_view = match wgpu.create_texture_from_image(&texture_label, image) {
				Ok((_, texture_view)) => texture_view,
				Err(err) => {
					tracing::warn!("Unable to create texture for image {:?}: {}", res.path, err.pretty());
					return;
				},
			};

			let image = PanelFadeImage {
				texture_view,
				bind_group: OnceCell::new(),
				geometry_uniforms: Mutex::new(HashMap::new()),
				swap_dir: rand::random(),
				path: res.path,
			};

			match slot {
				PanelFadeImageSlot::Prev => self.prev = Some(image),
				PanelFadeImageSlot::Cur => self.cur = Some(image),
				PanelFadeImageSlot::Next => self.next = Some(image),
			}
		}
	}

	/// Gets the next image, if any.
	///
	/// If an image is not scheduled, schedules it, even after
	/// successfully returning an image
	fn next_image(&mut self, playlist_player: &mut PlaylistPlayer, wgpu: &Wgpu) -> Option<ImageLoadRes> {
		// Schedule it and try to take any existing image result
		_ = self.schedule_load_image(playlist_player, wgpu);
		self.next_image.take()
	}

	/// Schedules a new image.
	///
	/// If the image is loaded, returns it
	fn schedule_load_image(&mut self, playlist_player: &mut PlaylistPlayer, wgpu: &Wgpu) -> Option<&mut ImageLoadRes> {
		// If we're loaded, just return it
		// Note: We can't use if-let due to a borrow-checker limitation
		if self.next_image.get().is_some() {
			return self.next_image.get_mut();
		}

		// Get the playlist position and path to load
		let (playlist_pos, path) = match () {
			() if self.cur.is_none() => (playlist_player.cur_pos(), playlist_player.cur()?),
			() if self.next.is_none() => (playlist_player.next_pos(), playlist_player.next()?),
			() if self.prev.is_none() => (playlist_player.prev_pos()?, playlist_player.prev()?),
			() => return None,
		};

		let max_image_size = wgpu.device.limits().max_texture_dimension_2d;

		self.next_image.try_load(|| {
			tokio::task::Builder::new()
				.name(&format!("Load image {path:?}"))
				.spawn_blocking(move || {
					let image_res = self::load(&path, max_image_size);
					ImageLoadRes {
						path,
						playlist_pos,
						image_res,
					}
				})
				.context("Unable to spawn task")
		})
	}

	/// Returns if all images are empty
	pub fn is_empty(&self) -> bool {
		self.prev.is_none() && self.cur.is_none() && self.next.is_none()
	}
}

/// Image slot
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, Debug)]
pub enum PanelFadeImageSlot {
	Prev,
	Cur,
	Next,
}

#[derive(Debug)]
pub struct ImageLoadRes {
	path:         Arc<Path>,
	playlist_pos: usize,
	image_res:    Result<DynamicImage, AppError>,
}

/// Loads an image
pub fn load(path: &Arc<Path>, max_image_size: u32) -> Result<DynamicImage, AppError> {
	// Load the image
	tracing::trace!("Loading image {:?}", path);
	#[cloned(path)]
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

	Ok(image)
}

/// Creates the fade image bind group layout
fn create_bind_group_layout(wgpu: &Wgpu) -> wgpu::BindGroupLayout {
	let descriptor = wgpu::BindGroupLayoutDescriptor {
		label:   Some("zsw-panel-fade-image-bind-group-layout"),
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

/// Creates the image bind group
fn create_image_bind_group(
	wgpu: &Wgpu,
	bind_group_layout: &wgpu::BindGroupLayout,
	view: &wgpu::TextureView,
	sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
	let descriptor = wgpu::BindGroupDescriptor {
		label:   Some("zsw-panel-fade-image-bind-group"),
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

/// Creates the geometry uniforms bind group layout
fn create_geometry_uniforms_bind_group_layout(wgpu: &Wgpu) -> wgpu::BindGroupLayout {
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

/// Panel fade geometry image uniforms
#[derive(Debug)]
pub struct PanelFadeImageGeometryUniforms {
	/// Buffer
	pub buffer: wgpu::Buffer,

	/// Bind group
	pub bind_group: wgpu::BindGroup,
}

/// Creates the image geometry uniforms
fn create_image_geometry_uniforms(wgpu: &Wgpu, shared: &PanelFadeImagesShared) -> PanelFadeImageGeometryUniforms {
	// Create the uniforms
	let buffer_descriptor = wgpu::BufferDescriptor {
		label:              Some("zsw-panel-fade-geometry-uniforms-buffer"),
		usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
		size:               u64::try_from(
			zsw_util::array_max(&[
				size_of::<uniform::FadeBasic>(),
				size_of::<uniform::FadeWhite>(),
				size_of::<uniform::FadeOut>(),
				size_of::<uniform::FadeIn>(),
			])
			.expect("No max uniform size"),
		)
		.expect("Maximum uniform size didn't fit into a `u64`"),
		mapped_at_creation: false,
	};
	let buffer = wgpu.device.create_buffer(&buffer_descriptor);

	// Create the uniform bind group
	let bind_group_descriptor = wgpu::BindGroupDescriptor {
		label:   Some("zsw-panel-fade-geometry-uniforms-bind-group"),
		layout:  &shared.geometry_uniforms_bind_group_layout,
		entries: &[wgpu::BindGroupEntry {
			binding:  0,
			resource: buffer.as_entire_binding(),
		}],
	};
	let bind_group = wgpu.device.create_bind_group(&bind_group_descriptor);

	PanelFadeImageGeometryUniforms { buffer, bind_group }
}

/// Creates the image sampler
fn create_image_sampler(wgpu: &Wgpu) -> wgpu::Sampler {
	let descriptor = wgpu::SamplerDescriptor {
		label: Some("zsw-panel-fade-image-sampler"),
		address_mode_u: wgpu::AddressMode::ClampToEdge,
		address_mode_v: wgpu::AddressMode::ClampToEdge,
		address_mode_w: wgpu::AddressMode::ClampToEdge,
		mag_filter: wgpu::FilterMode::Linear,
		min_filter: wgpu::FilterMode::Linear,
		mipmap_filter: wgpu::FilterMode::Linear,
		..wgpu::SamplerDescriptor::default()
	};
	wgpu.device.create_sampler(&descriptor)
}
