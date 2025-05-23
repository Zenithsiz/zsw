//! Panel image

// Imports
use {
	crate::image_loader::Image,
	cgmath::Vector2,
	image::DynamicImage,
	std::{path::PathBuf, sync::OnceLock},
	wgpu::util::DeviceExt,
	zsw_wgpu::WgpuShared,
};

/// Panel's image
///
/// Represents a single image of a panel.
#[derive(Debug)]
pub enum PanelImage {
	/// Empty
	Empty,

	/// Loaded
	Loaded {
		/// Texture
		_texture: wgpu::Texture,

		/// Texture view
		texture_view: wgpu::TextureView,

		/// Image size
		size: Vector2<u32>,

		/// Swap direction
		swap_dir: bool,

		/// Image path
		image_path: PathBuf,
	},
}

impl PanelImage {
	/// Creates a new, empty, panel image
	#[must_use]
	pub fn empty() -> Self {
		Self::Empty
	}

	/// Creates a new panel image from an image
	#[must_use]
	pub fn new(wgpu_shared: &WgpuShared, image: Image) -> Self {
		let size = Vector2::new(image.image.width(), image.image.height());
		let (texture, texture_view) = self::create_image_texture(wgpu_shared, image.image);

		Self::Loaded {
			_texture: texture,
			texture_view,
			size,
			swap_dir: rand::random(),
			image_path: image.path,
		}
	}

	/// Returns the texture view for this image.
	pub fn texture_view<'a>(&'a self, wgpu_shared: &'_ WgpuShared) -> &'a wgpu::TextureView {
		match self {
			Self::Empty => &self::get_empty_image_texture(wgpu_shared).1,
			Self::Loaded { texture_view, .. } => texture_view,
		}
	}

	/// Returns if this image is loaded
	pub fn is_loaded(&self) -> bool {
		matches!(self, Self::Loaded { .. })
	}
}

/// Empty texture
static EMPTY_TEXTURE: OnceLock<(wgpu::Texture, wgpu::TextureView)> = OnceLock::new();

/// Gets an empty texture
fn get_empty_image_texture(wgpu_shared: &WgpuShared) -> &'static (wgpu::Texture, wgpu::TextureView) {
	EMPTY_TEXTURE.get_or_init(|| {
		// TODO: Pass some view formats?
		let texture_descriptor =
			self::texture_descriptor("[zsw::panel] Null image", 1, 1, wgpu::TextureFormat::Rgba8UnormSrgb, &[
			]);
		let texture = wgpu_shared.device.create_texture(&texture_descriptor);
		let texture_view_descriptor = wgpu::TextureViewDescriptor::default();
		let texture_view = texture.create_view(&texture_view_descriptor);

		(texture, texture_view)
	})
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
