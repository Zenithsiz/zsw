//! Image

// Imports
use super::PanelUniforms;
use crate::{img::ImageUvs, util};
use cgmath::Vector2;
use image::{DynamicImage, GenericImageView};
use wgpu::util::DeviceExt;

/// Image
// TODO: Don't like that this stores the panel size and swap direction, check what we'll do
#[derive(Debug)]
pub struct PanelImage {
	/// Texture
	texture: wgpu::Texture,

	/// Texture view
	texture_view: wgpu::TextureView,

	/// Texture sampler
	texture_sampler: wgpu::Sampler,

	/// Texture bind group
	texture_bind_group: wgpu::BindGroup,

	/// Uniforms
	uniforms: wgpu::Buffer,

	/// Uniforms bind group
	uniforms_bind_group: wgpu::BindGroup,

	/// Image size
	image_size: Vector2<u32>,

	/// If we're swapping scrolling directions
	swap_dir: bool,
}

impl PanelImage {
	/// Creates a new image
	pub fn new(
		device: &wgpu::Device, queue: &wgpu::Queue, uniforms_bind_group_layout: &wgpu::BindGroupLayout,
		texture_bind_group_layout: &wgpu::BindGroupLayout, image: DynamicImage,
	) -> Result<Self, anyhow::Error> {
		// Create the texture and sampler
		let image_size = Vector2::new(image.width(), image.height());
		let (texture, texture_view) = self::create_image_texture(image, device, queue);
		let texture_sampler = self::create_texture_sampler(device);

		// Create the uniforms
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
			uniforms,
			uniforms_bind_group,
			image_size,
			swap_dir: rand::random(),
		})
	}

	/// Updates this image
	#[allow(clippy::unnecessary_wraps)] // It might fail in the future
	pub fn update(
		&mut self, device: &wgpu::Device, queue: &wgpu::Queue, texture_bind_group_layout: &wgpu::BindGroupLayout,
		image: DynamicImage,
	) -> Result<(), anyhow::Error> {
		// Update the image
		self.image_size = Vector2::new(image.width(), image.height());
		self.swap_dir = rand::random();

		// Then update our texture
		(self.texture, self.texture_view) = self::create_image_texture(image, device, queue);
		self.texture_bind_group = self::create_texture_bind_group(
			texture_bind_group_layout,
			&self.texture_view,
			&self.texture_sampler,
			device,
		);

		Ok(())
	}

	/// Returns this image's uvs for a panel size
	pub fn uvs(&self, panel_size: Vector2<u32>) -> ImageUvs {
		self::uvs(self.image_size, panel_size, self.swap_dir)
	}

	/// Updates this image's uniforms
	pub fn update_uniform(&self, queue: &wgpu::Queue, uniforms: PanelUniforms) {
		queue.write_buffer(&self.uniforms, 0, bytemuck::cast_slice(&[uniforms]));
	}

	/// Binds this image's uniforms and texture
	pub fn bind<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
		render_pass.set_bind_group(0, &self.uniforms_bind_group, &[]);
		render_pass.set_bind_group(1, &self.texture_bind_group, &[]);
	}
}

/// Returns the uvs for the panel
fn uvs(image_size: Vector2<u32>, panel_size: Vector2<u32>, swap_dir: bool) -> ImageUvs {
	ImageUvs::new(
		image_size.x as f32,
		image_size.y as f32,
		panel_size.x as f32,
		panel_size.y as f32,
		swap_dir,
	)
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
	image: DynamicImage, device: &wgpu::Device, queue: &wgpu::Queue,
) -> (wgpu::Texture, wgpu::TextureView) {
	// Get the image's format, converting if necessary.
	let (image, format) = match image {
		// With `rgba` we can simply use the image
		image @ DynamicImage::ImageRgba8(_) => (image, wgpu::TextureFormat::Rgba8UnormSrgb),

		// TODO: Don't convert more common formats (such as rgb8) if possible.
		
		// Else simply convert to rgba8
		image => {
			let old_format = util::image_format(&image);
			let (image, duration) = util::measure(move || image.into_rgba8());
			log::debug!(target: "zsw::perf", "Took {duration:?} to convert image to rgba (from {old_format})");
			(DynamicImage::ImageRgba8(image), wgpu::TextureFormat::Rgba8UnormSrgb)
		},
	};

	let texture_descriptor = self::texture_descriptor(image.width(), image.height(), format);
	let texture = device.create_texture_with_data(queue, &texture_descriptor, image.as_bytes());
	let texture_view_descriptor = wgpu::TextureViewDescriptor::default();
	let texture_view = texture.create_view(&texture_view_descriptor);
	(texture, texture_view)
}

/// Builds the texture descriptor
fn texture_descriptor(
	image_width: u32, image_height: u32, format: wgpu::TextureFormat,
) -> wgpu::TextureDescriptor<'static> {
	wgpu::TextureDescriptor {
		label: None,
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
