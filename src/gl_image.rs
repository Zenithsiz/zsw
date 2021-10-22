//! Image

use anyhow::Context;

// Imports
use crate::{image_loader::ImageLoader, ImageBuffer, ImageUvs, Vertex};


/// Image
#[derive(Debug)]
pub struct GlImage {
	/// Texture
	pub texture: glium::Texture2d,

	/// Uvs
	pub uvs: ImageUvs,

	/// Vertex buffer
	pub vertex_buffer: glium::VertexBuffer<Vertex>,

	/// Window size
	pub window_size: [u32; 2],
}

impl GlImage {
	/// Creates a new image
	///
	/// # Errors
	/// Returns error if unable to create the gl texture or the vertex buffer
	pub fn new(
		facade: &glium::Display, image: ImageBuffer, window_size @ [window_width, window_height]: [u32; 2],
	) -> Result<Self, anyhow::Error> {
		let image_dims = image.dimensions();
		let texture = glium::texture::Texture2d::new(
			facade,
			glium::texture::RawImage2d::from_raw_rgba(image.into_raw(), image_dims),
		)
		.context("Unable to create texture")?;

		#[allow(clippy::cast_precision_loss)] // Image and window sizes are likely much lower than 2^24
		let uvs = ImageUvs::new(
			image_dims.0 as f32,
			image_dims.1 as f32,
			window_width as f32,
			window_height as f32,
			rand::random(),
		);

		let vertex_buffer = glium::VertexBuffer::dynamic(facade, &Self::vertices(uvs.start()))
			.context("Unable to create vertex buffer")?;
		Ok(Self {
			texture,
			uvs,
			vertex_buffer,
			window_size,
		})
	}

	/// Tries to update this image and returns if actually updated
	///
	/// # Errors
	/// Returns error if unable to load an image or create a new gl texture
	pub fn try_update(
		&mut self, facade: &glium::Display, image_loader: &mut ImageLoader, force_wait: bool,
	) -> Result<bool, anyhow::Error> {
		let image = match image_loader
			.try_next_image()
			.context("Unable to try to get next image")?
		{
			Some(image) => image,
			None if force_wait => image_loader.next_image().context("Unable to get next image")?,
			None => return Ok(false),
		};

		let image_dims = image.dimensions();
		self.texture = glium::texture::Texture2d::new(
			facade,
			glium::texture::RawImage2d::from_raw_rgba(image.into_raw(), image_dims),
		)
		.context("Unable to create texture")?;

		#[allow(clippy::cast_precision_loss)] // Image and window sizes are likely much lower than 2^24
		let uvs = ImageUvs::new(
			image_dims.0 as f32,
			image_dims.1 as f32,
			self.window_size[0] as f32,
			self.window_size[1] as f32,
			rand::random(),
		);
		self.uvs = uvs;

		self.vertex_buffer
			.as_mut_slice()
			.write(&Self::vertices(self.uvs.start()));

		Ok(true)
	}

	/// Creates the vertices for uvs
	const fn vertices(uvs_start: [f32; 2]) -> [Vertex; 4] {
		[
			Vertex {
				vertex_pos: [-1.0, -1.0],
				vertex_tex: [0.0, 0.0],
			},
			Vertex {
				vertex_pos: [1.0, -1.0],
				vertex_tex: [uvs_start[0], 0.0],
			},
			Vertex {
				vertex_pos: [-1.0, 1.0],
				vertex_tex: [0.0, uvs_start[1]],
			},
			Vertex {
				vertex_pos: [1.0, 1.0],
				vertex_tex: uvs_start,
			},
		]
	}
}
