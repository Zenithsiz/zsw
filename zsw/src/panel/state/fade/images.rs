//! Panel fade images

use {
	crate::{panel::renderer::uniform, playlist::PlaylistPlayer},
	app_error::Context,
	core::clone::Share,
	image::imageops,
	std::{
		self,
		mem,
		path::Path,
		sync::{Arc, OnceLock},
	},
	zsw_util::{AppError, Loadable},
	zsw_wgpu::Wgpu,
};

/// Panel fade images shared
#[derive(Default, Debug)]
pub struct GeometryShared {
	/// Uniforms
	pub uniforms: Option<GeometryUniforms>,
}

impl GeometryShared {
	/// Returns the geometry uniforms
	pub fn uniforms(&mut self, wgpu: &Wgpu, shared: &Shared) -> &mut GeometryUniforms {
		self.uniforms
			.get_or_insert_with(|| self::create_image_geometry_uniforms(wgpu, shared))
	}
}

/// Panel fade images shared
#[derive(Debug)]
pub struct Shared {
	/// Geometry uniforms bind group layout
	pub geometry_uniforms_bind_group_layout: OnceLock<wgpu::BindGroupLayout>,

	/// Image bind group layout
	pub image_bind_group_layout: OnceLock<wgpu::BindGroupLayout>,
}

impl Shared {
	/// Creates the shared
	pub fn new() -> Self {
		Self {
			geometry_uniforms_bind_group_layout: OnceLock::new(),
			image_bind_group_layout:             OnceLock::new(),
		}
	}

	/// Gets the geometry uniforms bind group layout, or initializes it, if uninitialized
	pub fn geometry_uniforms_bind_group_layout(&self, wgpu: &Wgpu) -> &wgpu::BindGroupLayout {
		self.geometry_uniforms_bind_group_layout
			.get_or_init(|| self::create_geometry_uniforms_bind_group_layout(wgpu))
	}

	/// Gets the image bind group layout, or initializes it, if uninitialized
	pub fn image_bind_group_layout(&self, wgpu: &Wgpu) -> &wgpu::BindGroupLayout {
		self.image_bind_group_layout
			.get_or_init(|| self::create_bind_group_layout(wgpu))
	}
}

/// Panel fade images
#[derive(Debug)]
pub struct Images {
	/// Previous image
	pub prev: Option<Image>,

	/// Current image
	pub cur: Option<Image>,

	/// Next image
	pub next: Option<Image>,

	/// Image sampler
	pub image_sampler: OnceLock<wgpu::Sampler>,

	/// Bind group
	pub bind_group: OnceLock<wgpu::BindGroup>,

	/// Empty texture
	pub empty_texture_view: OnceLock<wgpu::TextureView>,

	/// Next image
	pub next_image: Loadable<ImageLoadRes>,
}

/// Panel's fade image
#[derive(Debug)]
pub struct Image {
	/// Texture view
	pub texture_view: wgpu::TextureView,

	/// Swap direction
	pub swap_dir: bool,

	/// Path
	pub path: Arc<Path>,
}

impl Images {
	/// Creates a new panel
	#[must_use]
	pub fn new() -> Self {
		Self {
			prev:               None,
			cur:                None,
			next:               None,
			image_sampler:      OnceLock::new(),
			bind_group:         OnceLock::new(),
			empty_texture_view: OnceLock::new(),
			next_image:         Loadable::new(),
		}
	}

	/// Steps to the previous image, if any
	///
	/// If successful, starts loading any missing images
	///
	/// Returns `Err(())` if this would erase the current image.
	pub fn step_prev(&mut self, playlist_player: &mut PlaylistPlayer, wgpu: &Arc<Wgpu>) -> Result<(), ()> {
		playlist_player.step_prev()?;
		mem::swap(&mut self.cur, &mut self.next);
		mem::swap(&mut self.prev, &mut self.cur);
		self.prev = None;
		self.bind_group = OnceLock::new();
		self.load_missing(playlist_player, wgpu);

		Ok(())
	}

	/// Steps to the next image.
	///
	/// If successful, starts loading any missing images
	///
	/// Returns `Err(())` if this would erase the current image.
	pub fn step_next(&mut self, playlist_player: &mut PlaylistPlayer, wgpu: &Arc<Wgpu>) -> Result<(), ()> {
		if self.next.is_none() {
			return Err(());
		}

		playlist_player.step_next();
		mem::swap(&mut self.prev, &mut self.cur);
		mem::swap(&mut self.cur, &mut self.next);
		self.next = None;
		self.bind_group = OnceLock::new();
		self.load_missing(playlist_player, wgpu);

		Ok(())
	}

	/// Gets the image sampler, or initializes it, if uninitialized
	pub fn image_sampler(&self, wgpu: &Wgpu) -> &wgpu::Sampler {
		self.image_sampler.get_or_init(|| self::create_image_sampler(wgpu))
	}

	/// Gets the bind group, or initializes it, if uninitialized
	pub fn bind_group(&self, wgpu: &Wgpu, sampler: &wgpu::Sampler, shared: &Shared) -> &wgpu::BindGroup {
		self.bind_group.get_or_init(|| {
			let [prev, cur, next] = [&self.prev, &self.cur, &self.next].map(|img| match img {
				Some(img) => &img.texture_view,
				None => self.empty_texture_view.get_or_init(|| {
					let (_texture, texture_view) = self::create_empty_image_texture(&wgpu.device);
					texture_view
				}),
			});

			let layout = shared.image_bind_group_layout(wgpu);
			self::create_image_bind_group(wgpu, layout, prev, cur, next, sampler)
		})
	}

	/// Loads any missing images, prioritizing the current, then next, then previous.
	///
	/// Requests images if missing any.
	pub fn load_missing(&mut self, playlist_player: &mut PlaylistPlayer, wgpu: &Arc<Wgpu>) {
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
				tracing::warn!("Unable to load image {:?}, removing it from player: {err:?}", res.path);
				playlist_player.remove(&res.path);

				_ = self.schedule_load_image(playlist_player, wgpu);
				return;
			},
		};

		// Get which slot to load the image into
		let slot = {
			let playlist_pos = playlist_player.cur_pos();
			match res.playlist_pos {
				pos if pos + 1 == playlist_pos => Some(ImageSlot::Prev),
				pos if pos == playlist_pos => Some(ImageSlot::Cur),
				pos if pos == playlist_pos + 1 => Some(ImageSlot::Next),
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
			match slot {
				ImageSlot::Prev => self.prev = Some(image),
				ImageSlot::Cur => self.cur = Some(image),
				ImageSlot::Next => self.next = Some(image),
			}
			self.bind_group = OnceLock::new();
		}
	}

	/// Gets the next image, if any.
	///
	/// If an image is not scheduled, schedules it, even after
	/// successfully returning an image
	fn next_image(&mut self, playlist_player: &mut PlaylistPlayer, wgpu: &Arc<Wgpu>) -> Option<ImageLoadRes> {
		// Schedule it and try to take any existing image result
		_ = self.schedule_load_image(playlist_player, wgpu);
		self.next_image.take()
	}

	/// Schedules a new image.
	///
	/// If the image is loaded, returns it
	fn schedule_load_image(
		&mut self,
		playlist_player: &mut PlaylistPlayer,
		wgpu: &Arc<Wgpu>,
	) -> Option<&mut ImageLoadRes> {
		// If we're loaded, just return it
		// Note: We can't use if-let due to a borrow-checker limitation
		if self.next_image.get().is_some() {
			return self.next_image.get_mut();
		}

		// Get the playlist position and path to load
		let (playlist_pos, path) = match () {
			() if self.cur.is_none() => playlist_player.get(0)?,
			() if self.next.is_none() => playlist_player.get(1)?,
			() if self.prev.is_none() => playlist_player.get(-1)?,
			() => return None,
		};

		let max_image_size = wgpu.device.limits().max_texture_dimension_2d;

		self.next_image.try_load(|tx| {
			let wgpu_shared = wgpu.share();
			zsw_util::spawn_task(format!("Load image {path:?}"), move || {
				let image_res = self::load(&wgpu_shared, &path, max_image_size);
				_ = tx.send(ImageLoadRes {
					path,
					playlist_pos,
					image_res,
				});

				Ok(())
			});
		})
	}

	/// Returns if all images are empty
	pub fn is_empty(&self) -> bool {
		self.prev.is_none() && self.cur.is_none() && self.next.is_none()
	}
}

/// Image slot
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, Debug)]
pub enum ImageSlot {
	Prev,
	Cur,
	Next,
}

#[derive(Debug)]
pub struct ImageLoadRes {
	path:         Arc<Path>,
	playlist_pos: usize,
	image_res:    Result<Image, AppError>,
}

/// Loads an image
pub fn load(wgpu: &Wgpu, path: &Arc<Path>, max_image_size: u32) -> Result<Image, AppError> {
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

	let texture_label = format!("zsw-panel-fade-image-texture[path={path:?}]");
	let (_texture, texture_view) = wgpu
		.create_texture_from_image(&texture_label, image)
		.context("Unable to create texture for image")?;

	let image = Image {
		texture_view,
		swap_dir: rand::random(),
		path: path.share(),
	};

	Ok(image)
}

/// Creates the fade image bind group layout
fn create_bind_group_layout(wgpu: &Wgpu) -> wgpu::BindGroupLayout {
	let entry = wgpu::BindGroupLayoutEntry {
		binding:    0,
		visibility: wgpu::ShaderStages::FRAGMENT,
		ty:         wgpu::BindingType::Texture {
			multisampled:   false,
			view_dimension: wgpu::TextureViewDimension::D2,
			sample_type:    wgpu::TextureSampleType::Float { filterable: true },
		},
		count:      None,
	};

	let descriptor = wgpu::BindGroupLayoutDescriptor {
		label:   Some("zsw-panel-fade-image-bind-group-layout"),
		entries: &[
			wgpu::BindGroupLayoutEntry { binding: 0, ..entry },
			wgpu::BindGroupLayoutEntry { binding: 1, ..entry },
			wgpu::BindGroupLayoutEntry { binding: 2, ..entry },
			wgpu::BindGroupLayoutEntry {
				binding:    3,
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
	prev_view: &wgpu::TextureView,
	cur_view: &wgpu::TextureView,
	next_view: &wgpu::TextureView,
	sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
	let descriptor = wgpu::BindGroupDescriptor {
		label:   Some("zsw-panel-fade-image-bind-group"),
		layout:  bind_group_layout,
		entries: &[
			wgpu::BindGroupEntry {
				binding:  0,
				resource: wgpu::BindingResource::TextureView(prev_view),
			},
			wgpu::BindGroupEntry {
				binding:  1,
				resource: wgpu::BindingResource::TextureView(cur_view),
			},
			wgpu::BindGroupEntry {
				binding:  2,
				resource: wgpu::BindingResource::TextureView(next_view),
			},
			wgpu::BindGroupEntry {
				binding:  3,
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
pub struct GeometryUniforms {
	/// Buffer
	pub buffer: wgpu::Buffer,

	/// Bind group
	pub bind_group: wgpu::BindGroup,
}

/// Creates the image geometry uniforms
fn create_image_geometry_uniforms(wgpu: &Wgpu, shared: &Shared) -> GeometryUniforms {
	// Create the uniforms
	let buffer_descriptor = wgpu::BufferDescriptor {
		label:              Some("zsw-panel-fade-geometry-uniforms-buffer"),
		usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
		size:               u64::try_from(
			zsw_util::array_max(&[size_of::<uniform::fade::Basic>(), size_of::<uniform::fade::Out>()])
				.expect("No max uniform size"),
		)
		.expect("Maximum uniform size didn't fit into a `u64`"),
		mapped_at_creation: false,
	};
	let buffer = wgpu.device.create_buffer(&buffer_descriptor);

	// Create the uniform bind group
	let bind_group_descriptor = wgpu::BindGroupDescriptor {
		label:   Some("zsw-panel-fade-geometry-uniforms-bind-group"),
		layout:  shared.geometry_uniforms_bind_group_layout(wgpu),
		entries: &[wgpu::BindGroupEntry {
			binding:  0,
			resource: buffer.as_entire_binding(),
		}],
	};
	let bind_group = wgpu.device.create_bind_group(&bind_group_descriptor);

	GeometryUniforms { buffer, bind_group }
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
		mipmap_filter: wgpu::MipmapFilterMode::Linear,
		..wgpu::SamplerDescriptor::default()
	};
	wgpu.device.create_sampler(&descriptor)
}

/// Gets an empty texture
fn create_empty_image_texture(device: &wgpu::Device) -> (wgpu::Texture, wgpu::TextureView) {
	// TODO: Pass some view formats?
	let texture_descriptor = wgpu::TextureDescriptor {
		label:           Some("zsw-panel-fade-empty-image"),
		size:            wgpu::Extent3d {
			width:                 1,
			height:                1,
			depth_or_array_layers: 1,
		},
		mip_level_count: 1,
		sample_count:    1,
		dimension:       wgpu::TextureDimension::D2,
		format:          wgpu::TextureFormat::Rgba8UnormSrgb,
		usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
		view_formats:    &[],
	};

	let texture = device.create_texture(&texture_descriptor);
	let texture_view_descriptor = wgpu::TextureViewDescriptor {
		label: Some("zsw-texture-empty-view"),
		..Default::default()
	};
	let texture_view = texture.create_view(&texture_view_descriptor);

	(texture, texture_view)
}
