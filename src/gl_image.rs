//! Image

// Imports
use crate::{
	img::{Image, ImageLoader, ImageReceiver, ImageRequest},
	ImageUvs, Vertex,
};
use anyhow::Context;
use cgmath::Vector2;
use std::{collections::VecDeque, time::Duration};

/// Image
#[derive(Debug)]
pub struct GlImage {
	/// Texture
	pub texture: glium::Texture2d,

	/// Uvs
	pub uvs: ImageUvs,

	/// Vertex buffer
	pub vertex_buffer: glium::VertexBuffer<Vertex>,

	/// All image receivers
	pub image_rxs: VecDeque<ImageReceiver>,

	/// Request
	pub request: ImageRequest,
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
		facade: &glium::Display, image_loader: &ImageLoader, window_size: Vector2<u32>, image_backlog: usize,
	) -> Result<Self, anyhow::Error> {
		let request = ImageRequest { window_size };

		let image = self::request(image_loader, request, Self::PRIORITY_HIGH)
			.recv()
			.context("Unable to get image")?;

		// Note: Make sure we have at least 1 receiver
		let image_rxs = (0..image_backlog)
			.map(|_| self::request(image_loader, request, Self::PRIORITY_LOW))
			.collect();

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
			image_rxs,
			request,
		})
	}

	/// Tries to update this image and returns if actually updated
	///
	/// # Errors
	/// Returns error if unable to load an image or create a new gl texture
	pub fn try_update(
		&mut self, facade: &glium::Display, image_loader: &ImageLoader, force_wait: bool,
	) -> Result<bool, anyhow::Error> {
		let image = match self::get_image(&mut self.image_rxs, image_loader, self.request, force_wait) {
			Some(image) => image,
			None => {
				assert!(!force_wait, "Received no image while force waiting");
				return Ok(false);
			},
		};

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
			self.request.window_size[0] as f32,
			self.request.window_size[1] as f32,
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

// TODO: Redo all of this, seems to cause some CPU pinning *sometimes*
fn get_image(
	image_rxs: &mut VecDeque<ImageReceiver>, image_loader: &ImageLoader, request: ImageRequest, force_wait: bool,
) -> Option<Image> {
	let timeout = Duration::from_millis(200); // TODO: Adjust timeout
	let mut cur_idx = 0;
	loop {
		// If we're not force waiting and we've gone through all image receivers,
		// quit
		if !force_wait && cur_idx >= image_rxs.len() {
			return None;
		}

		// Else pop the front receiver and try to receiver it
		let image_rx = image_rxs.pop_front().expect("No image receivers found");
		match image_rx.try_recv().expect("Unable to load next image") {
			// If we got it, create a new request and return the image
			Ok(image) => {
				let image_rx = self::request(image_loader, request, GlImage::PRIORITY_LOW);
				image_rxs.push_back(image_rx);
				return Some(image);
			},
			// Else push the receiver back
			Err(image_rx) => image_rxs.push_back(image_rx),
		}

		// If we're force waiting and we reached the end, sleep for a bit
		if force_wait && (cur_idx + 1) % image_rxs.len() == 0 {
			std::thread::sleep(timeout);
		}

		cur_idx += 1;
	}
}

/// Requests an image
fn request(image_loader: &ImageLoader, request: ImageRequest, priority: usize) -> ImageReceiver {
	image_loader
		.request(request, priority)
		.expect("Unable to request image")
}
