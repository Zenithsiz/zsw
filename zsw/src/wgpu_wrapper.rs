//! Wgpu wrapper

// Imports
use {
	anyhow::Context,
	std::{marker::PhantomData, sync::Arc},
	wgpu::TextureFormat,
	winit::{dpi::PhysicalSize, window::Window},
};

/// Wgpu shared
#[derive(Debug)]
pub struct WgpuShared {
	/// Device
	pub device: wgpu::Device,

	/// Queue
	pub queue: wgpu::Queue,

	/// Surface texture format
	pub surface_texture_format: TextureFormat,
}

/// Wgpu renderer
#[derive(Debug)]
pub struct WgpuRenderer {
	/// Surface
	surface: wgpu::Surface,

	/// Surface size
	// Note: We keep the size ourselves instead of using the inner
	//       window size because the window resizes asynchronously
	//       from us, so it's possible for the window sizes to be
	//       wrong relative to the surface size.
	//       Wgpu validation code can panic if the size we give it
	//       is invalid (for example, during scissoring), so we *must*
	//       ensure this size is the surface's actual size.
	surface_size: PhysicalSize<u32>,

	/// Window
	_window: Arc<Window>,
}

impl WgpuRenderer {
	/// Creates the wgpu renderer
	pub async fn new(window: Arc<Window>) -> Result<(Self, WgpuShared), anyhow::Error> {
		// Create the surface and adapter
		// SAFETY: We keep a reference to the window, to ensure it outlives the surface
		let (surface, adapter) = unsafe { self::create_surface_and_adapter(&window).await? };

		// Then create the device and it's queue
		let (device, queue) = self::create_device(&adapter).await?;

		// Configure the surface and get the preferred texture format and surface size
		let (surface_texture_format, surface_size) =
			self::configure_window_surface(&window, &surface, &adapter, &device)?;

		Ok((
			Self {
				surface,
				surface_size,
				_window: window,
			},
			WgpuShared {
				device,
				queue,
				surface_texture_format,
			},
		))
	}

	/// Returns the surface size
	pub fn surface_size(&self) -> PhysicalSize<u32> {
		self.surface_size
	}

	/// Starts rendering a frame.
	///
	/// Returns the encoder and surface view to render onto
	pub fn start_render(&mut self, shared: &WgpuShared) -> Result<FrameRender, anyhow::Error> {
		// And then get the surface texture
		// Note: This can block, so we run it under tokio's block-in-place
		let surface_texture = tokio::task::block_in_place(|| self.surface.get_current_texture())
			.context("Unable to retrieve current texture")?;
		let surface_view_descriptor = wgpu::TextureViewDescriptor {
			label: Some("[zsw] Window surface texture view"),
			..wgpu::TextureViewDescriptor::default()
		};
		let surface_texture_view = surface_texture.texture.create_view(&surface_view_descriptor);

		// Then create an encoder for our frame
		let encoder_descriptor = wgpu::CommandEncoderDescriptor {
			label: Some("[zsw] Frame render command encoder"),
		};
		let encoder = shared.device.create_command_encoder(&encoder_descriptor);

		Ok(FrameRender {
			encoder,
			surface_texture,
			surface_view: surface_texture_view,
			surface_size: self.surface_size,
			_phantom: PhantomData,
		})
	}

	/// Performs a resize
	pub fn resize(&mut self, shared: &WgpuShared, size: PhysicalSize<u32>) {
		tracing::info!(?size, "Resizing wgpu surface");
		if size.width > 0 && size.height > 0 {
			// Update our surface
			let config = self::window_surface_configuration(shared.surface_texture_format, size);
			self.surface.configure(&shared.device, &config);
			self.surface_size = size;
		}
	}
}

/// A frame's rendering
#[derive(Debug)]
pub struct FrameRender<'renderer> {
	/// Encoder
	pub encoder: wgpu::CommandEncoder,

	/// Surface texture
	pub surface_texture: wgpu::SurfaceTexture,

	/// Surface view
	pub surface_view: wgpu::TextureView,

	/// Surface size
	surface_size: PhysicalSize<u32>,

	/// Phantom data to prevent rendering more than a frame at once
	_phantom: PhantomData<&'renderer mut ()>,
}

impl<'renderer> FrameRender<'renderer> {
	/// Returns the surface size
	pub fn surface_size(&self) -> PhysicalSize<u32> {
		self.surface_size
	}

	/// Finishes rendering this frame
	pub fn finish(self, shared: &WgpuShared) {
		// Submit everything to the queue and present the surface's texture
		// Note: Although not supposed to, `submit` calls can block, so we wrap it
		//       in a tokio block-in-place
		let _ = tokio::task::block_in_place(|| shared.queue.submit([self.encoder.finish()]));
		//let _ = shared.queue.submit([self.encoder.finish()]);
		self.surface_texture.present();
	}
}

/// Configures the window surface and returns the preferred surface texture format
fn configure_window_surface(
	window: &Window,
	surface: &wgpu::Surface,
	adapter: &wgpu::Adapter,
	device: &wgpu::Device,
) -> Result<(TextureFormat, PhysicalSize<u32>), anyhow::Error> {
	// Get the format
	let surface_texture_format = *surface
		.get_supported_formats(adapter)
		.first()
		.context("No supported texture formats for surface found")?;
	tracing::debug!(?surface_texture_format, "Found preferred surface format");

	// Then configure it
	let surface_size = window.inner_size();
	let config = self::window_surface_configuration(surface_texture_format, surface_size);
	tracing::debug!(?config, "Configuring surface");
	surface.configure(device, &config);

	Ok((surface_texture_format, surface_size))
}

/// Creates the device
async fn create_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue), anyhow::Error> {
	// Request the device without any features
	let device_descriptor = wgpu::DeviceDescriptor {
		label:    Some("[zsw] Device"),
		features: wgpu::Features::empty(),
		limits:   wgpu::Limits::default(),
	};
	tracing::debug!(?device_descriptor, "Requesting wgpu device");
	let (device, queue) = adapter
		.request_device(&device_descriptor, None)
		.await
		.context("Unable to request device")?;

	// Configure the device to not panic on errors
	device.on_uncaptured_error(|err| {
		tracing::error!("Wgpu error: {err}");
	});

	Ok((device, queue))
}

/// Creates the surface and adapter
///
/// # Safety
/// The returned surface *must* be dropped before the window.
async unsafe fn create_surface_and_adapter(window: &Window) -> Result<(wgpu::Surface, wgpu::Adapter), anyhow::Error> {
	// Get an instance with any backend
	let backends = wgpu::Backends::all();
	tracing::debug!(?backends, "Requesting wgpu instance");
	let instance = wgpu::Instance::new(backends);
	tracing::debug!(?instance, "Created wgpu instance");

	// Create the surface
	// SAFETY: Caller promises the window outlives the surface
	tracing::debug!(?window, "Requesting wgpu surface");
	#[deny(unsafe_op_in_unsafe_fn)]
	let surface = unsafe { instance.create_surface(window) };
	tracing::debug!(?surface, "Created wgpu surface");

	// Then request the adapter
	let adapter_options = wgpu::RequestAdapterOptions {
		power_preference:       wgpu::PowerPreference::default(),
		force_fallback_adapter: false,
		compatible_surface:     Some(&surface),
	};
	tracing::debug!(?adapter_options, "Requesting wgpu adapter");
	let adapter = instance
		.request_adapter(&adapter_options)
		.await
		.context("Unable to request adapter")?;
	tracing::debug!(?adapter, "Created wgpu adapter");

	Ok((surface, adapter))
}

/// Returns the window surface configuration
const fn window_surface_configuration(
	surface_texture_format: TextureFormat,
	size: PhysicalSize<u32>,
) -> wgpu::SurfaceConfiguration {
	wgpu::SurfaceConfiguration {
		usage:        wgpu::TextureUsages::RENDER_ATTACHMENT,
		format:       surface_texture_format,
		width:        size.width,
		height:       size.height,
		present_mode: wgpu::PresentMode::AutoVsync,
		alpha_mode:   wgpu::CompositeAlphaMode::Auto,
	}
}
