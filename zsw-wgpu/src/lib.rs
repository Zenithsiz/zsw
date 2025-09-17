//! Wgpu wrapper

// Features
#![feature(must_not_suspend, yeet_expr)]

// Modules
mod renderer;

// Exports
pub use renderer::{FrameRender, WgpuRenderer};

// Imports
use {
	app_error::Context,
	core::sync::atomic::{self, AtomicBool},
	image::DynamicImage,
	wgpu::{util as wgpu_util, util::DeviceExt},
	zsw_util::AppError,
};

/// Wgpu
#[derive(Debug)]
pub struct Wgpu {
	/// Instance
	pub instance: wgpu::Instance,

	/// Adapter
	pub adapter: wgpu::Adapter,

	/// Device
	pub device: wgpu::Device,

	/// Queue
	pub queue: wgpu::Queue,

	// TODO: Move these out of here elsewhere? They're not necessary for wgpu, just
	//       for the panels.
	/// Empty texture
	pub empty_texture: wgpu::Texture,

	/// Empty texture view
	pub empty_texture_view: wgpu::TextureView,
}

impl Wgpu {
	/// Creates the wgpu.
	///
	/// # Panics
	/// Panics if called twice.
	pub async fn new() -> Result<Self, AppError> {
		static ALREADY_CREATED: AtomicBool = AtomicBool::new(false);
		assert!(
			!ALREADY_CREATED.swap(true, atomic::Ordering::AcqRel),
			"Cannot create a second wgpu instance"
		);

		let instance = self::create_instance().context("Unable to create instance")?;
		let adapter = self::create_adapter(&instance)
			.await
			.context("Unable to create adaptor")?;
		let (device, queue) = self::create_device(&adapter).await.context("Unable to create device")?;

		let (empty_texture, empty_texture_view) = self::create_empty_image_texture(&device);

		Ok::<_, AppError>(Self {
			instance,
			adapter,
			device,
			queue,
			empty_texture,
			empty_texture_view,
		})
	}

	/// Creates a texture from an image.
	pub fn create_texture_from_image(
		&self,
		label: &str,
		image: DynamicImage,
	) -> Result<(wgpu::Texture, wgpu::TextureView), AppError> {
		// Get the image's format, converting if necessary.
		let (image, format) = match image {
			// With `rgba8` we can simply use the image
			image @ DynamicImage::ImageRgba8(_) => (image, wgpu::TextureFormat::Rgba8UnormSrgb),

			// TODO: Convert more common formats (such as rgb8) if possible.

			// Else simply convert to rgba8
			image => {
				let image = image.to_rgba8();
				(DynamicImage::ImageRgba8(image), wgpu::TextureFormat::Rgba8UnormSrgb)
			},
		};

		// If the image is too large, return an error
		let limits = self.device.limits();
		let max_image_size = limits.max_texture_dimension_2d;
		let image_width = image.width();
		let image_height = image.height();
		app_error::ensure!(
			image_width <= max_image_size && image_height <= max_image_size,
			"Image is too large ({image_width}x{image_height}), maximum dimension is {max_image_size}",
		);

		let texture_descriptor = wgpu::TextureDescriptor {
			label: Some(label),
			size: wgpu::Extent3d {
				width:                 image.width(),
				height:                image.height(),
				depth_or_array_layers: 1,
			},
			mip_level_count: 1,
			sample_count: 1,
			dimension: wgpu::TextureDimension::D2,
			format,
			usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
			view_formats: &[],
		};

		let texture = self.device.create_texture_with_data(
			&self.queue,
			&texture_descriptor,
			wgpu_util::TextureDataOrder::LayerMajor,
			image.as_bytes(),
		);

		let texture_view_descriptor = wgpu::TextureViewDescriptor {
			label: Some(&format!("{label}-view")),
			..Default::default()
		};
		let texture_view = texture.create_view(&texture_view_descriptor);

		Ok((texture, texture_view))
	}
}

/// Creates the device
async fn create_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue), AppError> {
	// Request the device without any features
	let device_descriptor = wgpu::DeviceDescriptor {
		label:             Some("zsw-device"),
		required_features: wgpu::Features::default(),
		required_limits:   wgpu::Limits::default(),
		memory_hints:      wgpu::MemoryHints::default(),
		trace:             wgpu::Trace::Off,
	};
	tracing::debug!(?device_descriptor, "Requesting wgpu device");
	let (device, queue) = adapter
		.request_device(&device_descriptor)
		.await
		.context("Unable to request device")?;

	Ok((device, queue))
}

/// Creates the instance
fn create_instance() -> Result<wgpu::Instance, AppError> {
	let instance_desc = wgpu::InstanceDescriptor::from_env_or_default();
	tracing::debug!(?instance_desc, "Requesting wgpu instance");
	let instance = wgpu::Instance::new(&instance_desc);
	tracing::debug!(?instance, "Created wgpu instance");

	Ok(instance)
}

/// Creates the adapter
async fn create_adapter(instance: &wgpu::Instance) -> Result<wgpu::Adapter, AppError> {
	// Then request the adapter
	let adapter_options = wgpu::RequestAdapterOptions {
		power_preference:       wgpu::PowerPreference::default(),
		force_fallback_adapter: false,
		// TODO: Is this fine? Should we at least try to create this
		//       only after the first surface?
		compatible_surface:     None,
	};
	tracing::debug!(?adapter_options, "Requesting wgpu adapter");
	let adapter = instance
		.request_adapter(&adapter_options)
		.await
		.context("Unable to request adapter")?;
	tracing::debug!(?adapter, "Created wgpu adapter");

	Ok(adapter)
}

/// Gets an empty texture
fn create_empty_image_texture(device: &wgpu::Device) -> (wgpu::Texture, wgpu::TextureView) {
	// TODO: Pass some view formats?
	let texture_descriptor = wgpu::TextureDescriptor {
		label:           Some("zsw-texture-empty"),
		size:            wgpu::Extent3d {
			width:                 1,
			height:                1,
			depth_or_array_layers: 1,
		},
		mip_level_count: 1,
		sample_count:    1,
		dimension:       wgpu::TextureDimension::D2,
		format:          wgpu::TextureFormat::Rgba8UnormSrgb,
		usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
		view_formats:    &[],
	};

	let texture = device.create_texture(&texture_descriptor);
	let texture_view_descriptor = wgpu::TextureViewDescriptor {
		label: Some("zsw-texture-empty-view"),
		..Default::default()
	};
	let texture_view = texture.create_view(&texture_view_descriptor);

	(texture, texture_view)
}
