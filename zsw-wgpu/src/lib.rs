//! Wgpu wrapper

#![feature(yeet_expr)]
#![recursion_limit = "256"]

use {
	app_error::{Context, bail},
	euclid::default::Vector2D,
	image::DynamicImage,
	std::sync::Arc,
	wgpu::util::{self as wgpu_util, DeviceExt},
	zsw_util::AppError,
};

/// Wgpu shared
///
/// Wgpu data that can be shared.
#[derive(Debug)]
pub struct WgpuShared {
	/// Instance
	pub instance: wgpu::Instance,

	/// Adapter
	pub adapter: wgpu::Adapter,

	/// Device
	pub device: wgpu::Device,

	/// Queue
	pub queue: wgpu::Queue,

	/// Surface
	pub surface: wgpu::Surface<'static>,
}

impl WgpuShared {
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

/// Wgpu renderer
#[derive(Debug)]
pub struct WgpuRenderer {
	/// Wgpu shared
	pub shared: Arc<WgpuShared>,

	/// Surface config
	// Note: This is here instead of in shared because it needs
	pub surface_config: wgpu::SurfaceConfiguration,
}

impl WgpuRenderer {
	/// Creates the wgpu renderer
	pub async fn new(target: SurfaceTarget, surface_size: Vector2D<u32>) -> Result<Self, AppError> {
		let instance = self::create_instance().context("Unable to create instance")?;
		let surface = self::create_surface(&instance, target)?;

		let adapter = self::create_adapter(&instance, &surface)
			.await
			.context("Unable to create adaptor")?;
		let (device, queue) = self::create_device(&adapter).await.context("Unable to create device")?;

		// Configure the surface and get the preferred texture format and surface size
		let surface_config = self::configure_surface(&adapter, &device, &surface, surface_size)
			.context("Unable to configure surface")?;

		let shared = WgpuShared {
			instance,
			adapter,
			device,
			queue,
			surface,
		};

		Ok(Self {
			shared: Arc::new(shared),
			surface_config,
		})
	}

	/// Returns the surface size
	#[must_use]
	pub fn surface_size(&self) -> Vector2D<u32> {
		euclid::vec2(self.surface_config.width, self.surface_config.height)
	}

	/// Starts rendering a frame.
	///
	/// Returns the encoder and surface view to render onto
	// TODO: Ensure it's not called more than once?
	pub fn start_frame(&self) -> Result<FrameRender, AppError> {
		// And then get the surface texture
		let surface_texture = self.shared.surface.get_current_texture();
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
		let encoder = self.shared.device.create_command_encoder(&encoder_descriptor);

		Ok(FrameRender {
			encoder,
			surface_texture,
			surface_view: surface_texture_view,
			suboptimal,
		})
	}

	/// Submits all modifications of a frame.
	///
	/// Returns a rendered frame that can then be presented.
	pub fn submit_frame(&mut self, frame: FrameRender) -> Result<RenderedFrame, AppError> {
		_ = self.shared.queue.submit([frame.encoder.finish()]);

		Ok(RenderedFrame {
			surface_texture: frame.surface_texture,
			suboptimal:      frame.suboptimal,
		})
	}

	/// Presents a rendered frame.
	///
	/// Reconfigures if the frame is suboptimal
	pub fn present_frame(&mut self, frame: RenderedFrame) -> Result<(), AppError> {
		self.shared.queue.present(frame.surface_texture);

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
			self.surface_config.width,
			self.surface_config.height
		);

		// Update our surface
		self.surface_config = self::configure_surface(
			&self.shared.adapter,
			&self.shared.device,
			&self.shared.surface,
			self.surface_size(),
		)
		.context("Unable to configure surface")?;

		Ok(())
	}

	/// Performs a resize
	pub fn resize(&mut self, size: Vector2D<u32>) -> Result<(), AppError> {
		tracing::info!("Resizing wgpu surface to {}x{}", size.x, size.y);

		// TODO: Don't ignore resizes to the same size?
		if size.x > 0 && size.y > 0 && size != self.surface_size() {
			// Update our surface
			self.surface_config =
				self::configure_surface(&self.shared.adapter, &self.shared.device, &self.shared.surface, size)
					.context("Unable to configure surface")?;
		}

		Ok(())
	}
}

/// A frame's rendering
#[derive(Debug)]
#[must_use = "You must finish a frame render"]
pub struct FrameRender {
	/// Encoder
	pub encoder: wgpu::CommandEncoder,

	/// Surface texture
	pub surface_texture: wgpu::SurfaceTexture,

	/// Surface view
	pub surface_view: wgpu::TextureView,

	/// Whether the surface was sub-optimal
	pub suboptimal: bool,
}

/// A rendered frame
#[derive(Debug)]
#[must_use = "You must present a rendered frame"]
pub struct RenderedFrame {
	/// Surface texture
	pub surface_texture: wgpu::SurfaceTexture,

	/// Whether the surface was sub-optimal
	pub suboptimal: bool,
}

/// Configures the surface and returns the configuration
fn configure_surface(
	adapter: &wgpu::Adapter,
	device: &wgpu::Device,
	surface: &wgpu::Surface<'static>,
	size: Vector2D<u32>,
) -> Result<wgpu::SurfaceConfiguration, AppError> {
	let capabilities = surface.get_capabilities(adapter);
	tracing::debug!(?capabilities, "Found surface capabilities");

	// Get the format
	let mut config = surface
		.get_default_config(adapter, size.x, size.y)
		.context("Unable to get surface default config")?;
	tracing::debug!(?config, "Found surface configuration");

	// Set some options
	match capabilities.present_modes.contains(&wgpu::PresentMode::Mailbox) {
		true => {
			config.present_mode = wgpu::PresentMode::Mailbox;
			tracing::debug!("Using mailbox presentation for surface");
		},
		false => {
			config.present_mode = wgpu::PresentMode::AutoVsync;
			tracing::warn!("Mailbox presentation method is not supported, using fifo");
		},
	}

	// Then configure it
	surface.configure(device, &config);

	Ok(config)
}

/// Surface target kinds
#[derive(Debug)]
enum SurfaceTargetKind {
	WpuUnsafe(wgpu::SurfaceTargetUnsafe),
}

/// Surface target
#[derive(Debug)]
pub struct SurfaceTarget(SurfaceTargetKind);

impl SurfaceTarget {
	/// Creates a surface target from a wgpu unsafe target.
	///
	/// # Safety
	/// You must satisfy `SurfaceTargetUnsafe`'s safety requirements
	#[must_use]
	pub unsafe fn from_wgpu_unsafe(target: wgpu::SurfaceTargetUnsafe) -> Self {
		Self(SurfaceTargetKind::WpuUnsafe(target))
	}
}

/// Creates the surface
fn create_surface(instance: &wgpu::Instance, target: SurfaceTarget) -> Result<wgpu::Surface<'static>, AppError> {
	// Create the surface
	tracing::debug!(?target, "Requesting wgpu surface");
	let surface = match target.0 {
		SurfaceTargetKind::WpuUnsafe(target) => {
			// SAFETY: By creating a `SurfaceTargetKind::WgpuUnsafe` the caller has
			//         satisfied the safety requirements for the surface.
			unsafe { instance.create_surface_unsafe(target) }.context("Unable to request surface")?
		},
	};
	tracing::debug!(?surface, "Created wgpu surface");

	Ok(surface)
}

/// Creates the instance
fn create_instance() -> Result<wgpu::Instance, AppError> {
	let instance_desc = wgpu::InstanceDescriptor::new_without_display_handle();
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
