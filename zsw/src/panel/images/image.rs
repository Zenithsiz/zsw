//! Panel image

// Imports
use {
	cgmath::Vector2,
	image::DynamicImage,
	std::{path::Path, sync::Arc},
	wgpu::util::DeviceExt,
	zsw_wgpu::Wgpu,
};

/// Panel's fade image
///
/// Represents a single image of a panel.
#[derive(Debug)]
pub enum PanelFadeImage {
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
		image_path: Arc<Path>,
	},
}

impl PanelFadeImage {
	/// Creates a new, empty, panel image
	#[must_use]
	pub fn empty() -> Self {
		Self::Empty
	}

	/// Creates a new panel image from an image
	#[must_use]
	pub fn new(wgpu: &Wgpu, image_path: Arc<Path>, image: DynamicImage) -> Self {
		let size = Vector2::new(image.width(), image.height());
		let (texture, texture_view) = self::create_image_texture(wgpu, &image_path, image);

		Self::Loaded {
			_texture: texture,
			texture_view,
			size,
			swap_dir: rand::random(),
			image_path,
		}
	}

	/// Returns the texture view for this image.
	pub fn texture_view<'a>(&'a self, wgpu: &'a Wgpu) -> &'a wgpu::TextureView {
		match self {
			Self::Empty => &wgpu.empty_texture_view,
			Self::Loaded { texture_view, .. } => texture_view,
		}
	}

	/// Returns if this image is loaded
	pub fn is_loaded(&self) -> bool {
		matches!(self, Self::Loaded { .. })
	}
}

/// Creates the image texture and view
fn create_image_texture(wgpu: &Wgpu, image_path: &Path, image: DynamicImage) -> (wgpu::Texture, wgpu::TextureView) {
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
	let limits = wgpu.device.limits();
	let max_image_size = limits.max_texture_dimension_2d;
	let image_width = image.width();
	let image_height = image.height();
	assert!(
		image_width <= max_image_size && image_height <= max_image_size,
		"Loaded image was too big {image_width}x{image_height} (max: {max_image_size})",
	);

	let texture_descriptor = wgpu::TextureDescriptor {
		label: Some(&format!("[zsw::panel_img] Image ({image_path:?})")),
		size: wgpu::Extent3d {
			width:                 image.width(),
			height:                image.height(),
			depth_or_array_layers: 1,
		},
		mip_level_count: 1,
		sample_count: 1,
		dimension: wgpu::TextureDimension::D2,
		format,
		usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
		// TODO: Pass some view formats?
		view_formats: &[],
	};
	let texture = wgpu.device.create_texture_with_data(
		&wgpu.queue,
		&texture_descriptor,
		wgpu::util::TextureDataOrder::LayerMajor,
		image.as_bytes(),
	);
	let texture_view_descriptor = wgpu::TextureViewDescriptor::default();
	let texture_view = texture.create_view(&texture_view_descriptor);
	(texture, texture_view)
}
