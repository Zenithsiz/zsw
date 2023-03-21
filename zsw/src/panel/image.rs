//! Panel images

// Imports
use {
	super::{PanelGeometry, PanelsRendererLayouts, PlaylistPlayer},
	crate::image_loader::{Image, ImageReceiver, ImageRequest, ImageRequester},
	cgmath::Vector2,
	image::DynamicImage,
	std::{
		assert_matches::assert_matches,
		mem,
		path::{Path, PathBuf},
	},
	wgpu::util::DeviceExt,
	zsw_wgpu::WgpuShared,
};

/// Panel images
#[derive(Debug)]
pub struct PanelImages {
	/// Images state
	state: ImagesState,

	/// Front image
	front: PanelImage,

	/// Back image
	back: PanelImage,

	/// Texture sampler
	texture_sampler: wgpu::Sampler,

	/// Texture bind group
	image_bind_group: wgpu::BindGroup,

	/// Image receiver
	image_receiver: Option<ImageReceiver>,
}

impl PanelImages {
	/// Creates a new panel
	#[must_use]
	pub fn new(wgpu_shared: &WgpuShared, renderer_layouts: &PanelsRendererLayouts) -> Self {
		// Create the textures
		let front_image = PanelImage::new(wgpu_shared);
		let back_image = PanelImage::new(wgpu_shared);
		let texture_sampler = self::create_texture_sampler(wgpu_shared);
		let image_bind_group = self::create_image_bind_group(
			wgpu_shared,
			&renderer_layouts.image_bind_group_layout,
			&front_image.texture_view,
			&back_image.texture_view,
			&texture_sampler,
		);

		Self {
			state: ImagesState::Empty,
			front: front_image,
			back: back_image,
			texture_sampler,
			image_bind_group,
			image_receiver: None,
		}
	}

	/// Returns the current state of the images
	pub fn state(&self) -> ImagesState {
		self.state
	}

	/// Returns the front image
	pub fn front(&self) -> &PanelImage {
		&self.front
	}

	/// Returns the front image mutably
	pub fn front_mut(&mut self) -> &mut PanelImage {
		&mut self.front
	}

	/// Returns the back image
	pub fn back(&self) -> &PanelImage {
		&self.back
	}

	/// Returns the back image mutably
	pub fn back_mut(&mut self) -> &mut PanelImage {
		&mut self.back
	}

	/// Returns the image bind group for these images
	pub fn image_bind_group(&self) -> &wgpu::BindGroup {
		&self.image_bind_group
	}

	/// Swaps out the back with front and sets as only primary loaded
	pub fn swap_back(&mut self, wgpu_shared: &WgpuShared, renderer_layouts: &PanelsRendererLayouts) {
		match self.state {
			// If we're empty, there's nothing to swap
			ImagesState::Empty => (),

			// If we only have the primary, swapping puts us back to empty
			// Note: Since we're empty, there's no use in swapping the front and back buffers
			ImagesState::PrimaryOnly => {
				self.state = ImagesState::Empty;
			},

			// If we have both, swap the back and front
			ImagesState::Both => {
				self.state = ImagesState::PrimaryOnly;
				mem::swap(&mut self.front, &mut self.back);
				self.update_image_bind_group(wgpu_shared, renderer_layouts);
			},
		}
	}

	/// Advances to the next image, if available
	pub fn try_advance_next(
		&mut self,
		playlist_player: &mut PlaylistPlayer,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		image_requester: &ImageRequester,
		geometries: &[PanelGeometry],
	) {
		// If we have both images, don't load next
		if self.state == ImagesState::Both {
			return;
		}

		// Else try to load the next one
		if let Some(image) = self.try_load_next(wgpu_shared, playlist_player, image_requester, geometries) {
			// Then update the respective image and update the state
			self.state = match self.state {
				ImagesState::Empty => {
					self.front.update(wgpu_shared, image);
					ImagesState::PrimaryOnly
				},
				ImagesState::PrimaryOnly => {
					self.back.update(wgpu_shared, image);
					ImagesState::Both
				},
				// Note: If we were both, we would have quit above
				ImagesState::Both => unreachable!(),
			};

			self.update_image_bind_group(wgpu_shared, renderer_layouts);
		}
	}

	/// Tries to load the next image.
	///
	/// If unavailable, schedules it, and returns None.
	fn try_load_next(
		&mut self,
		wgpu_shared: &WgpuShared,
		playlist_player: &mut PlaylistPlayer,
		image_requester: &ImageRequester,
		geometries: &[PanelGeometry],
	) -> Option<Image> {
		match self.image_receiver.as_mut() {
			Some(image_receiver) => match image_receiver.try_recv() {
				Some(response) => {
					// Remove the exhausted receiver
					self.image_receiver = None;

					// Then check if we got the image
					match response.image_res {
						// If so, return it
						Ok(image) => Some(image),

						// Else, log an error, remove the image and re-schedule it
						Err(err) => {
							tracing::warn!(image_path = ?response.request.path, ?err, "Unable to load image, removing it from player");
							playlist_player.remove(&response.request.path);
							self.schedule_load_image(wgpu_shared, playlist_player, image_requester, geometries);
							None
						},
					}
				},
				None => None,
			},
			None => {
				self.schedule_load_image(wgpu_shared, playlist_player, image_requester, geometries);
				None
			},
		}
	}

	/// Schedules a new image
	fn schedule_load_image(
		&mut self,
		wgpu_shared: &WgpuShared,
		playlist_player: &mut PlaylistPlayer,
		image_requester: &ImageRequester,
		geometries: &[PanelGeometry],
	) {
		let image_path = match playlist_player.next() {
			Some(path) => path.to_path_buf(),
			None => {
				tracing::trace!("No images left");
				return;
			},
		};

		let wgpu_limits = wgpu_shared.device.limits();
		assert_matches!(self.image_receiver, None, "Overrode existing image loading future");
		self.image_receiver = Some(image_requester.request(ImageRequest {
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
			&self.front.texture_view,
			&self.back.texture_view,
			&self.texture_sampler,
		);
	}
}

/// State of all images of a panel
#[derive(PartialEq, Eq, Clone, Copy, Default, Debug)]
pub enum ImagesState {
	/// Empty
	///
	/// This means no images have been loaded yet
	#[default]
	Empty,

	/// Primary only
	///
	/// The primary image is loaded. The back image is still not available
	PrimaryOnly,

	/// Both
	///
	/// Both images are loaded to be faded in between
	Both,
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
			size: Vector2::new(0, 0),
			swap_dir: false,
			image_path: None,
		}
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
	}
}


/// Creates an empty texture
fn create_empty_image_texture(wgpu_shared: &WgpuShared) -> (wgpu::Texture, wgpu::TextureView) {
	let texture_descriptor =
		self::texture_descriptor("[zsw::panel] Null image", 1, 1, wgpu::TextureFormat::Rgba8UnormSrgb);
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

	let texture_descriptor = self::texture_descriptor("[zsw::panel_img] Image", image.width(), image.height(), format);
	let texture =
		wgpu_shared
			.device
			.create_texture_with_data(&wgpu_shared.queue, &texture_descriptor, image.as_bytes());
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
	front_view: &wgpu::TextureView,
	back_view: &wgpu::TextureView,
	sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
	let descriptor = wgpu::BindGroupDescriptor {
		layout:  bind_group_layout,
		entries: &[
			wgpu::BindGroupEntry {
				binding:  0,
				resource: wgpu::BindingResource::TextureView(front_view),
			},
			wgpu::BindGroupEntry {
				binding:  1,
				resource: wgpu::BindingResource::TextureView(back_view),
			},
			wgpu::BindGroupEntry {
				binding:  2,
				resource: wgpu::BindingResource::Sampler(sampler),
			},
		],
		label:   None,
	};
	wgpu_shared.device.create_bind_group(&descriptor)
}

/// Builds the texture descriptor
fn texture_descriptor(
	label: &str,
	width: u32,
	height: u32,
	format: wgpu::TextureFormat,
) -> wgpu::TextureDescriptor<'_> {
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
	}
}
