//! Panel images

// Imports
use {
	super::{PanelGeometry, PanelsRendererLayouts, PlaylistPlayer},
	crate::image_loader::{Image, ImageReceiver, ImageRequest, ImageRequester},
	cgmath::Vector2,
	image::DynamicImage,
	std::{
		mem,
		path::{Path, PathBuf},
	},
	tokio::sync::RwLock,
	wgpu::util::DeviceExt,
	zsw_wgpu::WgpuShared,
};

/// Panel images
#[derive(Debug)]
pub struct PanelImages {
	/// Previous image
	prev: PanelImage,

	/// Current image
	cur: PanelImage,

	/// Next image
	next: PanelImage,

	/// Texture sampler
	texture_sampler: wgpu::Sampler,

	/// Texture bind group
	image_bind_group: wgpu::BindGroup,

	/// Scheduled image receiver.
	scheduled_image_receiver: Option<ImageReceiver>,
}

impl PanelImages {
	/// Creates a new panel
	#[must_use]
	pub fn new(wgpu_shared: &WgpuShared, renderer_layouts: &PanelsRendererLayouts) -> Self {
		// Create the textures
		let image_prev = PanelImage::new(wgpu_shared);
		let image_cur = PanelImage::new(wgpu_shared);
		let image_next = PanelImage::new(wgpu_shared);
		let texture_sampler = self::create_texture_sampler(wgpu_shared);
		let image_bind_group = self::create_image_bind_group(
			wgpu_shared,
			&renderer_layouts.image_bind_group_layout,
			&image_prev.texture_view,
			&image_cur.texture_view,
			&image_next.texture_view,
			&texture_sampler,
		);

		Self {
			prev: image_prev,
			cur: image_cur,
			next: image_next,
			texture_sampler,
			image_bind_group,
			scheduled_image_receiver: None,
		}
	}

	/// Returns the image bind group for these images
	pub fn image_bind_group(&self) -> &wgpu::BindGroup {
		&self.image_bind_group
	}

	/// Steps to the next image
	pub fn step_next(&mut self, wgpu_shared: &WgpuShared, renderer_layouts: &PanelsRendererLayouts) {
		mem::swap(&mut self.prev, &mut self.cur);
		mem::swap(&mut self.cur, &mut self.next);
		self.next.is_loaded = false;
		self.update_image_bind_group(wgpu_shared, renderer_layouts);
	}

	/// Loads the next (or current) images.
	///
	/// Requests images if missing any.
	pub async fn load_next(
		&mut self,
		playlist_player: &RwLock<PlaylistPlayer>,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		image_requester: &ImageRequester,
		geometries: &[PanelGeometry],
	) {
		// Schedule the next image.
		self.schedule_load_image(wgpu_shared, playlist_player, image_requester, geometries)
			.await;

		// If we have both images, don't advance
		if self.next.is_loaded {
			return;
		}

		// Otherwise, try to load the image.
		if let Some(image) = self
			.load_img(wgpu_shared, playlist_player, image_requester, geometries)
			.await
		{
			match self.cur.is_loaded {
				true => self.next.update(wgpu_shared, image),
				false => self.cur.update(wgpu_shared, image),
			}
			self.update_image_bind_group(wgpu_shared, renderer_layouts);
		}
	}

	/// Tries to load the scheduled image.
	///
	/// If unavailable, schedules it, and returns None.
	async fn load_img(
		&mut self,
		wgpu_shared: &WgpuShared,
		playlist_player: &RwLock<PlaylistPlayer>,
		image_requester: &ImageRequester,
		geometries: &[PanelGeometry],
	) -> Option<Image> {
		// Get the image receiver, or schedule it.
		let Some(image_receiver) = self.scheduled_image_receiver.as_mut() else {
			self.schedule_load_image(wgpu_shared, playlist_player, image_requester, geometries)
				.await;
			return None;
		};

		// Then try to get the response
		let response = image_receiver.try_recv()?;

		// Remove the exhausted receiver
		self.scheduled_image_receiver = None;

		// Then check if we got the image
		match response.image_res {
			// If so, return it
			Ok(image) => Some(image),

			// Else, log an error, remove the image and re-schedule it
			Err(err) => {
				{
					tracing::warn!(image_path = ?response.request.path, ?err, "Unable to load image, removing it from player");
					let mut playlist_player = playlist_player.write().await;
					playlist_player.remove(&response.request.path);
				}

				self.schedule_load_image(wgpu_shared, playlist_player, image_requester, geometries)
					.await;
				None
			},
		}
	}

	/// Schedules a new image.
	///
	/// If the playlist player is empty, does not schedule.
	/// If already scheduled, returns
	async fn schedule_load_image(
		&mut self,
		wgpu_shared: &WgpuShared,
		playlist_player: &RwLock<PlaylistPlayer>,
		image_requester: &ImageRequester,
		geometries: &[PanelGeometry],
	) {
		if self.scheduled_image_receiver.is_some() {
			return;
		}

		let mut playlist_player = playlist_player.write().await;
		let image_path = match playlist_player.next() {
			Some(path) => path.to_path_buf(),
			None => {
				tracing::trace!("No images left");
				return;
			},
		};

		let wgpu_limits = wgpu_shared.device.limits();
		self.scheduled_image_receiver = Some(image_requester.request(ImageRequest {
			path:           image_path,
			geometries:     geometries.iter().map(|geometry| geometry.geometry).collect(),
			max_image_size: wgpu_limits.max_texture_dimension_2d,
		}));
	}

	/// Updates the image bind group
	fn update_image_bind_group(&mut self, wgpu_shared: &WgpuShared, renderer_layouts: &PanelsRendererLayouts) {
		self.image_bind_group = self::create_image_bind_group(
			wgpu_shared,
			&renderer_layouts.image_bind_group_layout,
			&self.prev.texture_view,
			&self.cur.texture_view,
			&self.next.texture_view,
			&self.texture_sampler,
		);
	}

	/// Returns the previous image
	pub fn prev(&self) -> &PanelImage {
		&self.prev
	}

	/// Returns the previous image mutably
	pub fn prev_mut(&mut self) -> &mut PanelImage {
		&mut self.prev
	}

	/// Returns the current image
	pub fn cur(&self) -> &PanelImage {
		&self.cur
	}

	/// Returns the current image mutably
	pub fn cur_mut(&mut self) -> &mut PanelImage {
		&mut self.cur
	}

	/// Returns the next image
	pub fn next(&self) -> &PanelImage {
		&self.next
	}

	/// Returns the next image mutably
	pub fn next_mut(&mut self) -> &mut PanelImage {
		&mut self.next
	}
}

/// Panel's image
///
/// Represents a single image of a panel.
#[derive(Debug)]
pub struct PanelImage {
	/// Texture
	texture: wgpu::Texture,

	/// Texture view
	texture_view: wgpu::TextureView,

	/// Whether the image is loaded
	is_loaded: bool,

	/// Image size
	size: Vector2<u32>,

	/// Swap direction
	swap_dir: bool,

	/// Image path
	image_path: Option<PathBuf>,
}

impl PanelImage {
	/// Creates a new image
	#[must_use]
	pub fn new(wgpu_shared: &WgpuShared) -> Self {
		// Create the texture and sampler
		let (texture, texture_view) = self::create_empty_image_texture(wgpu_shared);

		Self {
			texture,
			texture_view,
			is_loaded: false,
			size: Vector2::new(0, 0),
			swap_dir: false,
			image_path: None,
		}
	}

	/// Returns if this image is loaded
	pub fn is_loaded(&self) -> bool {
		self.is_loaded
	}

	/// Returns this image's size
	pub fn size(&self) -> Vector2<u32> {
		self.size
	}

	/// Returns the swap direction of this image
	pub fn swap_dir(&self) -> bool {
		self.swap_dir
	}

	/// Returns the swap direction of this image mutably
	pub fn swap_dir_mut(&mut self) -> &mut bool {
		&mut self.swap_dir
	}

	/// Returns the image path, if any
	pub fn path(&self) -> Option<&Path> {
		self.image_path.as_deref()
	}

	/// Updates this image
	pub fn update(&mut self, wgpu_shared: &WgpuShared, image: Image) {
		// Update our texture
		let size = Vector2::new(image.image.width(), image.image.height());
		(self.texture, self.texture_view) = self::create_image_texture(wgpu_shared, image.image);
		self.image_path = Some(image.path);

		// Then update the image size and swap direction
		self.size = size;
		self.swap_dir = rand::random();
		self.is_loaded = true;
	}
}


/// Creates an empty texture
fn create_empty_image_texture(wgpu_shared: &WgpuShared) -> (wgpu::Texture, wgpu::TextureView) {
	// TODO: Pass some view formats?
	let texture_descriptor =
		self::texture_descriptor("[zsw::panel] Null image", 1, 1, wgpu::TextureFormat::Rgba8UnormSrgb, &[
		]);
	let texture = wgpu_shared.device.create_texture(&texture_descriptor);
	let texture_view_descriptor = wgpu::TextureViewDescriptor::default();
	let texture_view = texture.create_view(&texture_view_descriptor);
	(texture, texture_view)
}

/// Creates the image texture and view
fn create_image_texture(wgpu_shared: &WgpuShared, image: DynamicImage) -> (wgpu::Texture, wgpu::TextureView) {
	// Get the image's format, converting if necessary.
	let (image, format) = match image {
		// With `rgba8` we can simply use the image
		image @ DynamicImage::ImageRgba8(_) => (image, wgpu::TextureFormat::Rgba8UnormSrgb),

		// TODO: Convert more common formats (such as rgb8) if possible.

		// Else simply convert to rgba8
		image => {
			let image = image.to_rgba8();
			(DynamicImage::ImageRgba8(image), wgpu::TextureFormat::Rgba8UnormSrgb)
		},
	};

	// Note: The image loader should ensure the image is the right size.
	let limits = wgpu_shared.device.limits();
	let max_image_size = limits.max_texture_dimension_2d;
	let image_width = image.width();
	let image_height = image.height();
	assert!(
		image_width <= max_image_size && image_height <= max_image_size,
		"Loaded image was too big {image_width}x{image_height} (max: {max_image_size})",
	);

	// TODO: Pass some view formats?
	let texture_descriptor =
		self::texture_descriptor("[zsw::panel_img] Image", image.width(), image.height(), format, &[]);
	let texture = wgpu_shared.device.create_texture_with_data(
		&wgpu_shared.queue,
		&texture_descriptor,
		wgpu::util::TextureDataOrder::LayerMajor,
		image.as_bytes(),
	);
	let texture_view_descriptor = wgpu::TextureViewDescriptor::default();
	let texture_view = texture.create_view(&texture_view_descriptor);
	(texture, texture_view)
}

/// Creates the texture sampler
fn create_texture_sampler(wgpu_shared: &WgpuShared) -> wgpu::Sampler {
	let descriptor = wgpu::SamplerDescriptor {
		label: Some("[zsw::panel] Texture sampler"),
		address_mode_u: wgpu::AddressMode::ClampToEdge,
		address_mode_v: wgpu::AddressMode::ClampToEdge,
		address_mode_w: wgpu::AddressMode::ClampToEdge,
		mag_filter: wgpu::FilterMode::Linear,
		min_filter: wgpu::FilterMode::Linear,
		mipmap_filter: wgpu::FilterMode::Linear,
		..wgpu::SamplerDescriptor::default()
	};
	wgpu_shared.device.create_sampler(&descriptor)
}

/// Creates the texture bind group
fn create_image_bind_group(
	wgpu_shared: &WgpuShared,
	bind_group_layout: &wgpu::BindGroupLayout,
	view_prev: &wgpu::TextureView,
	view_cur: &wgpu::TextureView,
	view_next: &wgpu::TextureView,
	sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
	let descriptor = wgpu::BindGroupDescriptor {
		layout:  bind_group_layout,
		entries: &[
			wgpu::BindGroupEntry {
				binding:  0,
				resource: wgpu::BindingResource::TextureView(view_prev),
			},
			wgpu::BindGroupEntry {
				binding:  1,
				resource: wgpu::BindingResource::TextureView(view_cur),
			},
			wgpu::BindGroupEntry {
				binding:  2,
				resource: wgpu::BindingResource::TextureView(view_next),
			},
			wgpu::BindGroupEntry {
				binding:  3,
				resource: wgpu::BindingResource::Sampler(sampler),
			},
		],
		label:   None,
	};
	wgpu_shared.device.create_bind_group(&descriptor)
}

/// Builds the texture descriptor
fn texture_descriptor<'a>(
	label: &'a str,
	width: u32,
	height: u32,
	format: wgpu::TextureFormat,
	view_formats: &'a [wgpu::TextureFormat],
) -> wgpu::TextureDescriptor<'a> {
	wgpu::TextureDescriptor {
		label: Some(label),
		size: wgpu::Extent3d {
			width,
			height,
			depth_or_array_layers: 1,
		},
		mip_level_count: 1,
		sample_count: 1,
		dimension: wgpu::TextureDimension::D2,
		format,
		usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
		view_formats,
	}
}
