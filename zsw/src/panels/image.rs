//! Image

// Imports
use {
	super::PanelUniforms,
	crate::{
		img::{Image, ImageUvs},
		util,
		PanelsRenderer,
		Wgpu,
	},
	cgmath::Vector2,
	image::{DynamicImage, GenericImageView},
	std::path::Path,
	wgpu::util::DeviceExt,
};

/// Panel's image
///
/// Represents a single image of a panel.
#[derive(Debug)]
pub struct PanelImage {
	/// Texture
	texture: wgpu::Texture,

	/// Texture view
	texture_view: wgpu::TextureView,

	/// Texture sampler
	texture_sampler: wgpu::Sampler,

	/// Texture bind group
	image_bind_group: wgpu::BindGroup,

	/// Uniforms
	uniforms: wgpu::Buffer,

	/// Uniforms bind group
	uniforms_bind_group: wgpu::BindGroup,

	/// Image size
	image_size: Vector2<u32>,
}

impl PanelImage {
	/// Creates a new image
	pub fn new(renderer: &PanelsRenderer, wgpu: &Wgpu, image: Image) -> Self {
		// Create the texture and sampler
		let image_size = image.size();
		let (texture, texture_view) = self::create_image_texture(wgpu, &image.path, image.image);
		let texture_sampler = self::create_texture_sampler(wgpu.device());

		// Create the uniforms
		// Note: Initial value doesn't matter
		let uniforms = PanelUniforms::default();
		let uniforms_descriptor = wgpu::util::BufferInitDescriptor {
			label:    None,
			contents: bytemuck::cast_slice(std::slice::from_ref(&uniforms)),
			usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
		};
		let uniforms = wgpu.device().create_buffer_init(&uniforms_descriptor);

		// Create the uniform bind group
		let uniforms_bind_group_descriptor = wgpu::BindGroupDescriptor {
			layout:  renderer.uniforms_bind_group_layout(),
			entries: &[wgpu::BindGroupEntry {
				binding:  0,
				resource: uniforms.as_entire_binding(),
			}],
			label:   None,
		};
		let uniforms_bind_group = wgpu.device().create_bind_group(&uniforms_bind_group_descriptor);

		// Create the texture bind group
		let image_bind_group = self::create_image_bind_group(
			wgpu,
			renderer.image_bind_group_layout(),
			&texture_view,
			&texture_sampler,
		);

		Self {
			texture,
			texture_view,
			texture_sampler,
			image_bind_group,
			uniforms,
			uniforms_bind_group,
			image_size,
		}
	}

	/// Updates this image
	pub fn update(&mut self, renderer: &PanelsRenderer, wgpu: &Wgpu, image: Image) {
		// Update the image
		self.image_size = image.size();

		// Then update our texture
		(self.texture, self.texture_view) = self::create_image_texture(wgpu, &image.path, image.image);
		self.image_bind_group = self::create_image_bind_group(
			wgpu,
			renderer.image_bind_group_layout(),
			&self.texture_view,
			&self.texture_sampler,
		);
	}

	/// Returns this image's uvs for a panel size
	pub fn uvs(&self, panel_size: Vector2<u32>, swap_dir: bool) -> ImageUvs {
		ImageUvs::new(
			self.image_size.x as f32,
			self.image_size.y as f32,
			panel_size.x as f32,
			panel_size.y as f32,
			swap_dir,
		)
	}

	/// Returns the uniforms buffer
	pub fn uniforms(&self) -> &wgpu::Buffer {
		&self.uniforms
	}

	/// Returns this image's uniforms bind group
	pub fn uniforms_bind_group(&self) -> &wgpu::BindGroup {
		&self.uniforms_bind_group
	}

	/// Returns this image's image bind group
	pub fn image_bind_group(&self) -> &wgpu::BindGroup {
		&self.image_bind_group
	}
}


/// Creates the texture sampler
fn create_texture_sampler(device: &wgpu::Device) -> wgpu::Sampler {
	let descriptor = wgpu::SamplerDescriptor {
		label: Some("[zsw::panel] Texture sampler"),
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
fn create_image_bind_group(
	wgpu: &Wgpu,
	bind_group_layout: &wgpu::BindGroupLayout,
	view: &wgpu::TextureView,
	sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
	let descriptor = wgpu::BindGroupDescriptor {
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

	wgpu.device().create_bind_group(&descriptor)
}

/// Creates the image texture and view
fn create_image_texture(wgpu: &Wgpu, path: &Path, image: DynamicImage) -> (wgpu::Texture, wgpu::TextureView) {
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

	let label = format!("[zsw::panel] Image {path:?}");
	let texture_descriptor = self::texture_descriptor(&label, image.width(), image.height(), format);
	let texture = wgpu
		.device()
		.create_texture_with_data(wgpu.queue(), &texture_descriptor, image.as_bytes());
	let texture_view_descriptor = wgpu::TextureViewDescriptor::default();
	let texture_view = texture.create_view(&texture_view_descriptor);
	(texture, texture_view)
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
