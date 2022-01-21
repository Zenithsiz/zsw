//! Image

// Imports
use crate::img::{Image, ImageLoader, ImageReceiver, ImageUvs};
use anyhow::Context;
use cgmath::Vector2;
use std::{collections::VecDeque, time::Duration};
use wgpu::util::DeviceExt;

use super::{PanelUniforms, PanelVertex};

/// Image
// TODO: Redo the whole image request stuff around here
#[derive(Debug)]
pub struct PanelImage {
	/// Texture
	pub texture: wgpu::Texture,

	/// Texture view
	pub texture_view: wgpu::TextureView,

	/// Texture sampler
	pub texture_sampler: wgpu::Sampler,

	/// Texture bind group
	pub texture_bind_group: wgpu::BindGroup,

	/// Vertices
	pub vertices: wgpu::Buffer,

	/// Uniforms
	pub uniforms: wgpu::Buffer,

	/// Uniforms bind group
	pub uniforms_bind_group: wgpu::BindGroup,

	/// Uvs
	pub uvs: ImageUvs,

	/// All image receivers
	pub image_rxs: VecDeque<ImageReceiver>,

	/// Panel size
	pub panel_size: Vector2<u32>,
}

impl PanelImage {
	/// High priority
	const PRIORITY_HIGH: usize = 1;
	/// Low priority
	const PRIORITY_LOW: usize = 0;

	/// Creates a new image
	///
	/// # Errors
	/// Returns error if unable to create the gl texture or the vertex buffer
	pub fn new(
		device: &wgpu::Device, queue: &wgpu::Queue, uniforms_bind_group_layout: &wgpu::BindGroupLayout,
		texture_bind_group_layout: &wgpu::BindGroupLayout, image_loader: &ImageLoader, panel_size: Vector2<u32>,
		image_backlog: usize,
	) -> Result<Self, anyhow::Error> {
		// Get the initial image
		let image = self::request(image_loader, Self::PRIORITY_HIGH)
			.recv()
			.context("Unable to get image")?;

		// Then start requesting images in the background
		// Note: Make sure we have at least 1 receiver
		let image_rxs = (0..image_backlog.min(1))
			.map(|_| self::request(image_loader, Self::PRIORITY_LOW))
			.collect();

		// Create the texture and sampler
		let (texture, texture_view) = self::create_image_texture(&image, device, queue);
		let texture_sampler = create_texture_sampler(device);

		let uvs = ImageUvs::new(
			image.width() as f32,
			image.height() as f32,
			panel_size.x as f32,
			panel_size.y as f32,
			rand::random(),
		);

		let vertices = Self::vertices(uvs.start());
		let vertex_buffer_descriptor = wgpu::util::BufferInitDescriptor {
			label:    None,
			contents: bytemuck::cast_slice(&vertices),
			usage:    wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
		};
		let vertices = device.create_buffer_init(&vertex_buffer_descriptor);

		let uniforms = PanelUniforms::new();
		let uniforms_descriptor = wgpu::util::BufferInitDescriptor {
			label:    None,
			contents: bytemuck::cast_slice(std::slice::from_ref(&uniforms)),
			usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
		};
		let uniforms = device.create_buffer_init(&uniforms_descriptor);

		// Create the uniform bind group
		let uniforms_bind_group_descriptor = wgpu::BindGroupDescriptor {
			layout:  uniforms_bind_group_layout,
			entries: &[wgpu::BindGroupEntry {
				binding:  0,
				resource: uniforms.as_entire_binding(),
			}],
			label:   None,
		};
		let uniforms_bind_group = device.create_bind_group(&uniforms_bind_group_descriptor);

		// Create the texture bind group
		let texture_bind_group =
			self::create_texture_bind_group(texture_bind_group_layout, &texture_view, &texture_sampler, device);

		Ok(Self {
			texture,
			texture_view,
			texture_sampler,
			texture_bind_group,
			vertices,
			uniforms,
			uniforms_bind_group,
			uvs,
			image_rxs,
			panel_size,
		})
	}

	/// Tries to update this image and returns if actually updated
	#[allow(clippy::unnecessary_wraps)] // It might fail in the future
	pub fn try_update(
		&mut self, device: &wgpu::Device, queue: &wgpu::Queue, texture_bind_group_layout: &wgpu::BindGroupLayout,
		image_loader: &ImageLoader, force_wait: bool,
	) -> Result<bool, anyhow::Error> {
		let image = match self::get_image(&mut self.image_rxs, image_loader, force_wait) {
			Some(image) => image,
			None => {
				debug_assert!(!force_wait, "Received no image while force waiting");
				return Ok(false);
			},
		};

		// Then update our texture
		(self.texture, self.texture_view) = self::create_image_texture(&image, device, queue);
		self.texture_bind_group = self::create_texture_bind_group(
			texture_bind_group_layout,
			&self.texture_view,
			&self.texture_sampler,
			device,
		);

		// Re-create our UVs
		self.uvs = ImageUvs::new(
			image.width() as f32,
			image.height() as f32,
			self.panel_size[0] as f32,
			self.panel_size[1] as f32,
			rand::random(),
		);

		// And update the vertex buffer
		queue.write_buffer(
			&self.vertices,
			0,
			bytemuck::cast_slice(&Self::vertices(self.uvs.start())),
		);

		Ok(true)
	}

	/// Creates the vertices for uvs
	const fn vertices(uvs_start: [f32; 2]) -> [PanelVertex; 4] {
		[
			PanelVertex {
				pos: [-1.0, -1.0],
				uvs: [0.0, 0.0],
			},
			PanelVertex {
				pos: [1.0, -1.0],
				uvs: [uvs_start[0], 0.0],
			},
			PanelVertex {
				pos: [-1.0, 1.0],
				uvs: [0.0, uvs_start[1]],
			},
			PanelVertex {
				pos: [1.0, 1.0],
				uvs: uvs_start,
			},
		]
	}

	/// Updates this image's uniforms
	pub fn update_uniform(&self, queue: &wgpu::Queue, uniforms: PanelUniforms) {
		queue.write_buffer(&self.uniforms, 0, bytemuck::cast_slice(&[uniforms]));
	}

	/// Binds this image's vertices and bind group
	pub fn bind<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
		render_pass.set_vertex_buffer(0, self.vertices.slice(..));
		render_pass.set_bind_group(0, &self.uniforms_bind_group, &[]);
		render_pass.set_bind_group(1, &self.texture_bind_group, &[]);
	}
}

/// Creates the texture sampler
fn create_texture_sampler(device: &wgpu::Device) -> wgpu::Sampler {
	let descriptor = wgpu::SamplerDescriptor {
		address_mode_u: wgpu::AddressMode::ClampToEdge,
		address_mode_v: wgpu::AddressMode::ClampToEdge,
		address_mode_w: wgpu::AddressMode::ClampToEdge,
		mag_filter: wgpu::FilterMode::Linear,
		min_filter: wgpu::FilterMode::Linear,
		mipmap_filter: wgpu::FilterMode::Linear,
		..wgpu::SamplerDescriptor::default()
	};
	device.create_sampler(&descriptor)
}

/// Creates the texture bind group
fn create_texture_bind_group(
	bind_group_layout: &wgpu::BindGroupLayout, view: &wgpu::TextureView, sampler: &wgpu::Sampler, device: &wgpu::Device,
) -> wgpu::BindGroup {
	let texture_bind_group_descriptor = wgpu::BindGroupDescriptor {
		layout:  bind_group_layout,
		entries: &[
			wgpu::BindGroupEntry {
				binding:  0,
				resource: wgpu::BindingResource::TextureView(view),
			},
			wgpu::BindGroupEntry {
				binding:  1,
				resource: wgpu::BindingResource::Sampler(sampler),
			},
		],
		label:   None,
	};

	device.create_bind_group(&texture_bind_group_descriptor)
}

/// Creates the image texture and view
fn create_image_texture(
	image: &Image, device: &wgpu::Device, queue: &wgpu::Queue,
) -> (wgpu::Texture, wgpu::TextureView) {
	let texture_descriptor = self::texture_descriptor(image.width(), image.height());
	let texture = device.create_texture_with_data(queue, &texture_descriptor, image.as_raw());
	let texture_view_descriptor = wgpu::TextureViewDescriptor::default();
	let texture_view = texture.create_view(&texture_view_descriptor);
	(texture, texture_view)
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
fn get_image(image_rxs: &mut VecDeque<ImageReceiver>, image_loader: &ImageLoader, force_wait: bool) -> Option<Image> {
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
				let image_rx = self::request(image_loader, PanelImage::PRIORITY_LOW);
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
fn request(image_loader: &ImageLoader, priority: usize) -> ImageReceiver {
	image_loader.request(priority).expect("Unable to request image")
}
