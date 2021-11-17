//! Image

use anyhow::Context;
use cgmath::Vector2;

// Imports
use crate::{
	image_loader::{ImageLoader, ImageReceiver},
	ImageUvs, Vertex,
};

/// Image
#[derive(Debug)]
pub struct GlImage {
	/// Texture
	pub texture: glium::Texture2d,

	/// Uvs
	pub uvs: ImageUvs,

	/// Vertex buffer
	pub vertex_buffer: glium::VertexBuffer<Vertex>,

	/// Next image receiver
	pub next_image_receiver: Option<ImageReceiver>,

	/// Window size
	pub window_size: Vector2<u32>,
}

impl GlImage {
	/// Creates a new image
	///
	/// # Errors
	/// Returns error if unable to create the gl texture or the vertex buffer
	pub fn new(
		facade: &glium::Display, _image_backlog: usize, image_loader: &ImageLoader, window_size: Vector2<u32>,
	) -> Result<Self, anyhow::Error> {
		let image = image_loader
			.queue(window_size)
			.context("Unable to queue image")?
			.recv(image_loader)
			.context("Unable to get image")?;

		let next_image_receiver = image_loader.queue(window_size).context("Unable to queue next image")?;

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
			window_size.x as f32,
			window_size.y as f32,
			rand::random(),
		);

		let vertex_buffer = glium::VertexBuffer::dynamic(facade, &Self::vertices(uvs.start()))
			.context("Unable to create vertex buffer")?;
		Ok(Self {
			texture,
			uvs,
			vertex_buffer,
			next_image_receiver: Some(next_image_receiver),
			window_size,
		})
	}

	/// Tries to update this image and returns if actually updated
	///
	/// # Errors
	/// Returns error if unable to load an image or create a new gl texture
	pub fn try_update(
		&mut self, facade: &glium::Display, image_loader: &ImageLoader, force_wait: bool,
	) -> Result<bool, anyhow::Error> {
		// Get the next image receiver, or create a new one, if we don't have any
		let cur_image_receiver = match self.next_image_receiver.take() {
			Some(receiver) => receiver,
			None => image_loader.queue(self.window_size).context("Unable to queue image")?,
		};

		// Try to get the next image
		let image = match force_wait {
			true => cur_image_receiver
				.recv(image_loader)
				.context("Unable to get next image")?,
			false => match cur_image_receiver
				.try_recv(image_loader)
				.context("Unable to get next image")?
			{
				Ok(image) => image,
				Err(receiver) => {
					self.next_image_receiver = Some(receiver);
					return Ok(false);
				},
			},
		};

		// Then queue up another image if we got the current one
		// Note: By here, we know for sure we don't have a receiver currently
		let next_image_receiver = image_loader
			.queue(self.window_size)
			.context("Unable to queue next image")?;
		self.next_image_receiver = Some(next_image_receiver);

		// Then update our texture
		let image_dims = image.dimensions();
		self.texture = glium::texture::Texture2d::new(
			facade,
			glium::texture::RawImage2d::from_raw_rgba(image.into_raw(), image_dims),
		)
		.context("Unable to create texture")?;

		// Re-create our UVs
		#[allow(clippy::cast_precision_loss)] // Image and window sizes are likely much lower than 2^24
		let uvs = ImageUvs::new(
			image_dims.0 as f32,
			image_dims.1 as f32,
			self.window_size[0] as f32,
			self.window_size[1] as f32,
			rand::random(),
		);
		self.uvs = uvs;

		// And update the vertex buffer
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
