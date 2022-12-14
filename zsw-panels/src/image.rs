//! Image

// Imports
use {
	cgmath::Vector2,
	image::DynamicImage,
	std::borrow::Cow,
	wgpu::util::DeviceExt,
	zsw_img::{Image, RawImageProvider},
	zsw_wgpu::Wgpu,
};

/// Panel's image
///
/// Represents a single image of a panel.
#[derive(Debug)]
pub struct PanelImage {
	/// Texture
	pub texture: wgpu::Texture,

	/// Texture view
	pub texture_view: wgpu::TextureView,

	/// Image size
	pub size: Vector2<u32>,

	/// Name
	pub name: String,
}

impl PanelImage {
	/// Creates a new image
	#[must_use]
	pub fn new(wgpu: &Wgpu) -> Self {
		// Create the texture and sampler
		let (texture, texture_view) = self::create_empty_image_texture(wgpu);

		Self {
			texture,
			texture_view,
			size: Vector2::new(0, 0),
			name: String::new(),
		}
	}

	/// Updates this image
	pub fn update<P: RawImageProvider>(
		&mut self,
		wgpu: &Wgpu,
		image: &Image<P>,
		max_image_size: Option<u32>,
	) -> Result<(), ImageTooBigError> {
		// Update our texture
		(self.texture, self.texture_view) =
			self::create_image_texture(wgpu, &image.name, &image.image, max_image_size)?;

		// Then update the image size and name
		self.size = image.size();
		self.name = image.name.clone();

		Ok(())
	}
}


/// Error when an image is too large
#[derive(Clone, Copy, Debug)]
#[derive(thiserror::Error)]
#[error("Image was too big: {image_width}x{image_height} max {max_image_size}x{max_image_size}")]
pub struct ImageTooBigError {
	/// Image width
	pub image_width: u32,

	/// Image height
	pub image_height: u32,

	/// Max size
	pub max_image_size: u32,
}

/// Creates an empty texture
fn create_empty_image_texture(wgpu: &Wgpu) -> (wgpu::Texture, wgpu::TextureView) {
	let texture_descriptor =
		self::texture_descriptor("[zsw::panel] Null image", 1, 1, wgpu::TextureFormat::Rgba8UnormSrgb);
	let texture = wgpu.device().create_texture(&texture_descriptor);
	let texture_view_descriptor = wgpu::TextureViewDescriptor::default();
	let texture_view = texture.create_view(&texture_view_descriptor);
	(texture, texture_view)
}

/// Creates the image texture and view
fn create_image_texture(
	wgpu: &Wgpu,
	name: &str,
	image: &DynamicImage,
	max_image_size: Option<u32>,
) -> Result<(wgpu::Texture, wgpu::TextureView), ImageTooBigError> {
	// Get the image's format, converting if necessary.
	let (image, format) = match image {
		// With `rgba` we can simply use the image
		image @ DynamicImage::ImageRgba8(_) => (Cow::Borrowed(image), wgpu::TextureFormat::Rgba8UnormSrgb),

		// TODO: Don't convert more common formats (such as rgb8) if possible.

		// Else simply convert to rgba8
		image => {
			let image = image.to_rgba8();
			(
				Cow::Owned(DynamicImage::ImageRgba8(image)),
				wgpu::TextureFormat::Rgba8UnormSrgb,
			)
		},
	};

	// If the image is too large, return Err
	let limits = wgpu.device().limits();
	let max_image_size = max_image_size
		.unwrap_or(limits.max_texture_dimension_2d)
		.min(limits.max_texture_dimension_2d);
	if image.width() > max_image_size || image.height() > max_image_size {
		return Err(ImageTooBigError {
			image_width: image.width(),
			image_height: image.height(),
			max_image_size,
		});
	}

	let label = format!("[zsw::panel] Image {name:?}");
	let texture_descriptor = self::texture_descriptor(&label, image.width(), image.height(), format);
	let texture = wgpu
		.device()
		.create_texture_with_data(wgpu.queue(), &texture_descriptor, image.as_bytes());
	let texture_view_descriptor = wgpu::TextureViewDescriptor::default();
	let texture_view = texture.create_view(&texture_view_descriptor);
	Ok((texture, texture_view))
}

/// Builds the texture descriptor
fn texture_descriptor(
	label: &str,
	image_width: u32,
	image_height: u32,
	format: wgpu::TextureFormat,
) -> wgpu::TextureDescriptor<'_> {
	wgpu::TextureDescriptor {
		label: Some(label),
		size: wgpu::Extent3d {
			width:                 image_width,
			height:                image_height,
			depth_or_array_layers: 1,
		},
		mip_level_count: 1,
		sample_count: 1,
		dimension: wgpu::TextureDimension::D2,
		format,
		usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
	}
}
