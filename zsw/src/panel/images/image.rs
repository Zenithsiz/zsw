//! Panel image

// Imports
use {
	app_error::Context,
	cgmath::Vector2,
	image::DynamicImage,
	std::{path::Path, sync::Arc},
	zsw_util::AppError,
	zsw_wgpu::Wgpu,
};

/// Panel's fade image
///
/// Represents a single image of a panel.
#[derive(Debug)]
pub struct PanelFadeImage {
	/// Texture
	pub _texture: wgpu::Texture,

	/// Texture view
	pub texture_view: wgpu::TextureView,

	/// Image size
	pub size: Vector2<u32>,

	/// Swap direction
	pub swap_dir: bool,

	/// Path
	pub path: Arc<Path>,
}

impl PanelFadeImage {
	/// Creates a new panel image from an image
	pub fn new(wgpu: &Wgpu, path: Arc<Path>, image: DynamicImage) -> Result<Self, AppError> {
		let size = Vector2::new(image.width(), image.height());
		let (texture, texture_view) = wgpu
			.create_texture_from_image(&path, image)
			.context("Unable to create texture from image")?;

		Ok(Self {
			_texture: texture,
			texture_view,
			size,
			swap_dir: rand::random(),
			path,
		})
	}
}
