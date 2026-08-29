//! Wgpu wrapper

#![feature(must_not_suspend, yeet_expr, share_trait)]

use {
	app_error::{Context, bail},
	core::clone::Share,
	image::DynamicImage,
	std::sync::Arc,
	wgpu::util::{self as wgpu_util, DeviceExt},
	winit::{dpi::PhysicalSize, event_loop::OwnedDisplayHandle, window::Window},
	zsw_util::AppError,
};

/// Wgpu renderer
#[derive(Debug)]
pub struct WgpuRenderer {
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

	/// Surface
	pub surface: wgpu::Surface<'static>,

	/// Surface size
	// Note: We keep the size ourselves instead of using the inner
	//       window size because the window resizes asynchronously
	//       from us, so it's possible for the window sizes to be
	//       wrong relative to the surface size.
	//       Wgpu validation code can panic if the size we give it
	//       is invalid (for example, during scissoring), so we *must*
	//       ensure this size is the surface's actual size.
	pub surface_size: PhysicalSize<u32>,

	/// Surface config
	pub surface_config: wgpu::SurfaceConfiguration,
}

impl WgpuRenderer {
	pub async fn new(display: OwnedDisplayHandle, force_opengl: bool, window: &Arc<Window>) -> Result<Self, AppError> {
		let instance = self::create_instance(display, force_opengl).context("Unable to create instance")?;
		let surface = self::create_surface(&instance, window.share())?;

		let adapter = self::create_adapter(&instance, &surface)
			.await
			.context("Unable to create adaptor")?;
		let (device, queue) = self::create_device(&adapter).await.context("Unable to create device")?;

		let (empty_texture, empty_texture_view) = self::create_empty_image_texture(&device);


		// Configure the surface and get the preferred texture format and surface size
		let surface_size = window.inner_size();
		let surface_config = self::configure_window_surface(&adapter, &device, &surface, surface_size)
			.context("Unable to configure window surface")?;

		Ok(Self {
			instance,
			adapter,
			device,
			queue,
			empty_texture,
			empty_texture_view,
			surface,
			surface_size,
			surface_config,
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

	/// Starts rendering a frame.
	///
	/// Returns the encoder and surface view to render onto
	// TODO: Ensure it's not called more than once?
	pub fn start_render(&self) -> Result<FrameRender, AppError> {
		// And then get the surface texture
		let surface_texture = self.surface.get_current_texture();
		let surface_view_descriptor = wgpu::TextureViewDescriptor {
			label: Some("zsw-frame-surface-texture-view"),
			..wgpu::TextureViewDescriptor::default()
		};
		let suboptimal = matches!(surface_texture, wgpu::CurrentSurfaceTexture::Suboptimal(_));
		let (surface_texture, surface_texture_view) = match surface_texture {
			wgpu::CurrentSurfaceTexture::Success(surface_texture) |
			wgpu::CurrentSurfaceTexture::Suboptimal(surface_texture) => {
				let surface_view = surface_texture.texture.create_view(&surface_view_descriptor);
				(surface_texture, surface_view)
			},

			err @ (wgpu::CurrentSurfaceTexture::Timeout |
			wgpu::CurrentSurfaceTexture::Occluded |
			wgpu::CurrentSurfaceTexture::Outdated |
			wgpu::CurrentSurfaceTexture::Lost |
			wgpu::CurrentSurfaceTexture::Validation) => bail!("Unable to get surface texture: {err:?}"),
		};

		// Then create an encoder for our frame
		let encoder_descriptor = wgpu::CommandEncoderDescriptor {
			label: Some("zsw-frame-command-encoder"),
		};
		let encoder = self.device.create_command_encoder(&encoder_descriptor);

		Ok(FrameRender {
			encoder,
			surface_texture,
			surface_view: surface_texture_view,
			surface_size: self.surface_size,
			suboptimal,
		})
	}

	/// Finishes rendering a frame.
	///
	/// Reconfigures if the frame as suboptimal
	pub fn finish_render(&mut self, frame: FrameRender) -> Result<(), AppError> {
		// Submit everything to the queue and present the surface's texture
		_ = self.queue.submit([frame.encoder.finish()]);
		self.queue.present(frame.surface_texture);

		if frame.suboptimal {
			self.reconfigure()
				.context("Unable to reconfigure wgpu after a suboptimal frame")?;
		}

		Ok(())
	}

	/// Re-configures the surface
	pub fn reconfigure(&mut self) -> Result<(), AppError> {
		tracing::info!(
			"Reconfiguring wgpu surface to {}x{}",
			self.surface_size.width,
			self.surface_size.height
		);

		// Update our surface
		self.surface_config =
			self::configure_window_surface(&self.adapter, &self.device, &self.surface, self.surface_size)
				.context("Unable to configure window surface")?;

		Ok(())
	}

	/// Performs a resize
	pub fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), AppError> {
		tracing::info!(
			"Resizing wgpu surface to {}x{}",
			self.surface_size.width,
			self.surface_size.height
		);

		// TODO: Don't ignore resizes to the same size?
		if size.width > 0 && size.height > 0 && size != self.surface_size {
			// Update our surface
			self.surface_config = self::configure_window_surface(&self.adapter, &self.device, &self.surface, size)
				.context("Unable to configure window surface")?;
			self.surface_size = size;
		}

		Ok(())
	}
}

/// A frame's rendering
#[derive(Debug)]
pub struct FrameRender {
	/// Encoder
	pub encoder: wgpu::CommandEncoder,

	/// Surface texture
	pub surface_texture: wgpu::SurfaceTexture,

	/// Surface view
	pub surface_view: wgpu::TextureView,

	/// Surface size
	pub surface_size: PhysicalSize<u32>,

	/// Whether the surface was sub-optimal
	pub suboptimal: bool,
}

/// Configures the window surface and returns the configuration
fn configure_window_surface(
	adapter: &wgpu::Adapter,
	device: &wgpu::Device,
	surface: &wgpu::Surface<'static>,
	size: PhysicalSize<u32>,
) -> Result<wgpu::SurfaceConfiguration, AppError> {
	// Get the format
	let mut config = surface
		.get_default_config(adapter, size.width, size.height)
		.context("Unable to get surface default config")?;
	tracing::debug!(?config, "Found surface configuration");

	// Set some options
	config.present_mode = wgpu::PresentMode::AutoVsync;
	tracing::debug!(?config, "Updated surface configuration");

	// Then configure it
	surface.configure(device, &config);

	Ok(config)
}

/// Creates the surface
fn create_surface(instance: &wgpu::Instance, window: Arc<Window>) -> Result<wgpu::Surface<'static>, AppError> {
	// Create the surface
	tracing::debug!(?window, "Requesting wgpu surface");
	let surface = instance.create_surface(window).context("Unable to request surface")?;
	tracing::debug!(?surface, "Created wgpu surface");

	Ok(surface)
}

/// Creates the instance
fn create_instance(display: OwnedDisplayHandle, force_opengl: bool) -> Result<wgpu::Instance, AppError> {
	let mut instance_desc = wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(display));
	if force_opengl {
		instance_desc.backends = wgpu::Backends::GL;
	}
	tracing::debug!(?instance_desc, "Requesting wgpu instance");
	let instance = wgpu::Instance::new(instance_desc);
	tracing::debug!(?instance, "Created wgpu instance");

	Ok(instance)
}

/// Creates the device
async fn create_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue), AppError> {
	// Request the device without any features
	let device_descriptor = wgpu::DeviceDescriptor {
		label:                 Some("zsw-device"),
		required_features:     wgpu::Features::default(),
		required_limits:       wgpu::Limits::default(),
		memory_hints:          wgpu::MemoryHints::default(),
		trace:                 wgpu::Trace::Off,
		experimental_features: wgpu::ExperimentalFeatures::default(),
	};
	tracing::debug!(?device_descriptor, "Requesting wgpu device");
	let (device, queue) = adapter
		.request_device(&device_descriptor)
		.await
		.context("Unable to request device")?;

	Ok((device, queue))
}

/// Creates the adapter
async fn create_adapter(
	instance: &wgpu::Instance,
	surface: &wgpu::Surface<'static>,
) -> Result<wgpu::Adapter, AppError> {
	// Then request the adapter
	let adapter_options = wgpu::RequestAdapterOptions {
		power_preference:       wgpu::PowerPreference::default(),
		force_fallback_adapter: false,
		compatible_surface:     Some(surface),
		apply_limit_buckets:    false,
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
