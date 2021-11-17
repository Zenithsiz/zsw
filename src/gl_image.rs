//! Image

use anyhow::Context;
use cgmath::Vector2;

// Imports
use crate::{
	image_loader::{self, ImageLoader},
	sync::once_channel,
	ImageBuffer, ImageUvs, Vertex,
};


/// Image receiver
type ImageReceiver = once_channel::Receiver<Result<ImageBuffer, image_loader::ResponseError>>;

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
		facade: &glium::Display, image_loader: &ImageLoader, window_size: Vector2<u32>,
	) -> Result<Self, anyhow::Error> {
		let image = Self::next_image(None, window_size, image_loader).context("Unable to load image")?;

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
			next_image_receiver: Some(
				Self::next_image_receiver(window_size, image_loader).context("Unable to queue next image")?,
			),
			window_size,
		})
	}

	/// Creates the next image receiver
	fn next_image_receiver(
		window_size: Vector2<u32>, image_loader: &ImageLoader,
	) -> Result<once_channel::Receiver<Result<ImageBuffer, image_loader::ResponseError>>, anyhow::Error> {
		image_loader.queue_image(window_size)
	}

	/// Tries to return the next image
	fn next_image(
		mut receiver: Option<ImageReceiver>, window_size: Vector2<u32>, image_loader: &ImageLoader,
	) -> Result<ImageBuffer, anyhow::Error> {
		loop {
			match receiver
				.take()
				.map_or_else(|| Self::next_image_receiver(window_size, image_loader), Ok)
				.context("Unable to queue next image")?
				.recv()
			{
				// If we got the image, return it
				Ok(Ok(image)) => break Ok(image),

				// If we got an error, inform the image loader and try again.
				Ok(Err(err)) => image_loader.on_failed_request(&err),

				Err(_) => anyhow::bail!("Unable to get next image"),
			}
		}
	}

	/// Returns the next image
	fn try_next_image(
		receiver: &mut Option<ImageReceiver>, window_size: Vector2<u32>, image_loader: &ImageLoader, force_wait: bool,
	) -> Result<Option<ImageBuffer>, anyhow::Error> {
		loop {
			let recv =
				|receiver: once_channel::Receiver<Result<ImageBuffer, image_loader::ResponseError>>| match force_wait {
					true => receiver.recv().map_err(|_| once_channel::TryRecvError::SenderQuit),
					false => receiver.try_recv(),
				};

			match recv(
				receiver
					.take()
					.map_or_else(|| Self::next_image_receiver(window_size, image_loader), Ok)
					.context("Unable to queue next image")?,
			) {
				// If we got the image, return it
				Ok(Ok(image)) => break Ok(Some(image)),

				// If we got an error, inform the image loader and try again.
				Ok(Err(err)) => {
					image_loader.on_failed_request(&err);
					continue;
				},

				// If it isn't ready, save the receiver and retry if we're force waiting
				Err(once_channel::TryRecvError::NotReady(new_receiver)) => {
					*receiver = Some(new_receiver);
					match force_wait {
						true => continue,
						false => break Ok(None),
					}
				},
				Err(_) => anyhow::bail!("Unable to get next image"),
			}
		}
	}

	/// Tries to update this image and returns if actually updated
	///
	/// # Errors
	/// Returns error if unable to load an image or create a new gl texture
	pub fn try_update(
		&mut self, facade: &glium::Display, image_loader: &ImageLoader, force_wait: bool,
	) -> Result<bool, anyhow::Error> {
		let image = Self::try_next_image(
			&mut self.next_image_receiver,
			self.window_size,
			image_loader,
			force_wait,
		)
		.context("Unable to get next image")?;
		let image = match image {
			Some(image) => image,
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
