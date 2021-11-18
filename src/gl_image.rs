//! Image

// Imports
use crate::{
	image_loader::{ImageLoader, ImageReceiver},
	ImageUvs, Vertex,
};
use anyhow::Context;
use cgmath::Vector2;
use std::collections::VecDeque;

/// Image
#[derive(Debug)]
pub struct GlImage {
	/// Texture
	pub texture: glium::Texture2d,

	/// Uvs
	pub uvs: ImageUvs,

	/// Vertex buffer
	///
	/// This should always have `image_backlog` elements (except
	/// when being modified)
	pub vertex_buffer: glium::VertexBuffer<Vertex>,

	/// Next image receivers
	pub next_image_receivers: VecDeque<ImageReceiver>,

	/// Window size
	pub window_size: Vector2<u32>,
}

impl GlImage {
	/// High priority
	const PRIORITY_HIGH: usize = 1;
	/// Low priority
	const PRIORITY_LOW: usize = 0;

	/// Creates a new image
	///
	/// # Errors
	/// Returns error if unable to create the gl texture or the vertex buffer
	pub fn new(
		facade: &glium::Display, image_backlog: usize, image_loader: &ImageLoader, window_size: Vector2<u32>,
	) -> Result<Self, anyhow::Error> {
		let image = image_loader
			.queue(window_size, Self::PRIORITY_HIGH)
			.context("Unable to queue image")?
			.recv(image_loader, Self::PRIORITY_HIGH)
			.context("Unable to get image")?;

		// Note: Make sure we have at least 1 receiver
		let next_image_receivers = (0..image_backlog.max(1))
			.map(|_| image_loader.queue(window_size, Self::PRIORITY_LOW))
			.collect::<Result<_, _>>()
			.context("Unable to queue next image")?;

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
			next_image_receivers,
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
		// Get the first loaded image
		let mut cur_idx = 0;
		let image = loop {
			// Get the receiver
			// Note: We know for sure we have at least 1 receiver
			#[allow(clippy::expect_used)]
			let receiver = self.next_image_receivers.pop_front().expect("No receivers");

			// If we've already looped once, check if we need to force wait on the next one
			// TODO: Use some facility to be able to `recv` all of them instead of
			//       just on the first one.
			if cur_idx > self.next_image_receivers.len() {
				match force_wait {
					true => {
						break receiver
							.recv(image_loader, Self::PRIORITY_HIGH)
							.context("Unable to get next image")?
					},
					false => {
						self.next_image_receivers.push_back(receiver);
						return Ok(false);
					},
				}
			}

			// Then try to receive it
			match receiver
				.try_recv(image_loader, Self::PRIORITY_LOW)
				.context("Unable to get next image")?
			{
				Ok(image) => break image,
				Err(receiver) => self.next_image_receivers.push_back(receiver),
			}

			cur_idx += 1;
		};

		// Then queue up another
		let next_image_receiver = image_loader
			.queue(self.window_size, Self::PRIORITY_LOW)
			.context("Unable to queue next image")?;
		self.next_image_receivers.push_back(next_image_receiver);

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
