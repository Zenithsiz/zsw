//! Image

// Imports
use crate::{
	img::{Image, ImageLoader, ImageReceiver, ImageRequest, ImageUvs},
	renderer::Vertex,
};
use anyhow::Context;
use cgmath::Vector2;
use std::{collections::VecDeque, time::Duration};
use wgpu::util::DeviceExt;

/// Image
#[derive(Debug)]
pub struct GlImage {
	/// Texture
	pub texture: wgpu::Texture,

	/// Uvs
	pub uvs: ImageUvs,

	/// Vertices
	pub vertices: wgpu::Buffer,

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
		device: &wgpu::Device, queue: &wgpu::Queue, image_loader: &ImageLoader, window_size: Vector2<u32>,
		image_backlog: usize,
	) -> Result<Self, anyhow::Error> {
		let request = ImageRequest { window_size };

		let image = self::request(image_loader, request, Self::PRIORITY_HIGH)
			.recv()
			.context("Unable to get image")?;

		// Note: Make sure we have at least 1 receiver
		let image_rxs = (0..image_backlog)
			.map(|_| self::request(image_loader, request, Self::PRIORITY_LOW))
			.collect();

		let (image_width, image_height) = image.dimensions();
		let texture_descriptor = self::texture_descriptor(image_width, image_height);
		let texture = device.create_texture_with_data(queue, &texture_descriptor, image.as_raw());

		#[allow(clippy::cast_precision_loss)] // Image and window sizes are likely much lower than 2^24
		let uvs = ImageUvs::new(
			image_width as f32,
			image_height as f32,
			window_size.x as f32,
			window_size.y as f32,
			rand::random(),
		);

		let vertices = Self::vertices(uvs.start());
		let vertex_buffer_descriptor = wgpu::util::BufferInitDescriptor {
			label:    None,
			contents: bytemuck::cast_slice(&vertices),
			usage:    wgpu::BufferUsages::VERTEX,
		};
		let vertices = device.create_buffer_init(&vertex_buffer_descriptor);

		Ok(Self {
			texture,
			uvs,
			vertices,
			image_rxs,
			request,
		})
	}

	/// Tries to update this image and returns if actually updated
	///
	/// # Errors
	/// Returns error if unable to load an image or create a new gl texture
	pub fn try_update(
		&mut self, device: &wgpu::Device, queue: &wgpu::Queue, image_loader: &ImageLoader, force_wait: bool,
	) -> Result<bool, anyhow::Error> {
		let image = match self::get_image(&mut self.image_rxs, image_loader, self.request, force_wait) {
			Some(image) => image,
			None => {
				debug_assert!(!force_wait, "Received no image while force waiting");
				return Ok(false);
			},
		};

		// Then update our texture
		let (image_width, image_height) = image.dimensions();
		let texture_descriptor = self::texture_descriptor(image_width, image_height);
		self.texture = device.create_texture_with_data(queue, &texture_descriptor, image.as_raw());

		// Re-create our UVs
		#[allow(clippy::cast_precision_loss)] // Image and window sizes are likely much lower than 2^24
		let uvs = ImageUvs::new(
			image_width as f32,
			image_height as f32,
			self.request.window_size[0] as f32,
			self.request.window_size[1] as f32,
			rand::random(),
		);
		self.uvs = uvs;

		// And update the vertex buffer
		self.vertices
			.slice(..)
			.get_mapped_range_mut()
			.copy_from_slice(bytemuck::cast_slice(&Self::vertices(self.uvs.start())));

		Ok(true)
	}

	/// Creates the vertices for uvs
	const fn vertices(uvs_start: [f32; 2]) -> [Vertex; 4] {
		[
			Vertex {
				pos: [-1.0, -1.0],
				uvs: [0.0, 0.0],
			},
			Vertex {
				pos: [1.0, -1.0],
				uvs: [uvs_start[0], 0.0],
			},
			Vertex {
				pos: [-1.0, 1.0],
				uvs: [0.0, uvs_start[1]],
			},
			Vertex {
				pos: [1.0, 1.0],
				uvs: uvs_start,
			},
		]
	}
}

/// Builds the texture descriptor
fn texture_descriptor(image_width: u32, image_height: u32) -> wgpu::TextureDescriptor<'static> {
	wgpu::TextureDescriptor {
		label:           None,
		size:            wgpu::Extent3d {
			width:                 image_width,
			height:                image_height,
			depth_or_array_layers: 1,
		},
		mip_level_count: 1,
		sample_count:    1,
		dimension:       wgpu::TextureDimension::D2,
		format:          wgpu::TextureFormat::Rgba8UnormSrgb,
		usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
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
