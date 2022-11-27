//! Wgpu
//!
//! This module serves as a high-level interface for `wgpu`.
//!
//! The main entry point is the [`Wgpu`] type, which is used
//! to interface with `wgpu`.
//!
//! This allows the application to not be exposed to the verbose
//! details of `wgpu` and simply use the defaults [`Wgpu`] offers,
//! which are tailored for this application.

// Imports
use {
	anyhow::Context,
	std::sync::Arc,
	wgpu::TextureFormat,
	winit::{dpi::PhysicalSize, window::Window},
	zsw_input::InputReceiver,
};

/// Wgpu interface
// TODO: Figure out if drop order matters here. Dropping the surface after the device/queue
//       seems to not result in any panics, but it might be worth checking, especially if we
//       ever need to "restart" `wgpu` in any scenario without restarting the application.
#[derive(Debug)]
pub struct Wgpu {
	/// Device
	// TODO: There exists a `Device::poll` method, but I'm not sure if we should
	//       have to call that? Seems to be used for async, but we don't use any
	//       of the async methods and unfortunately the polling seems to be busy
	//       waiting even in `Wait` mode, so creating a new thread to poll whenever
	//       doesn't work well without a sleep, which would defeat the point of
	//       polling it in another thread instead of on the main thread whenever
	//       an event is received.
	device: wgpu::Device,

	/// Queue
	queue: wgpu::Queue,

	/// Surface texture format
	///
	/// Used on each resize, so we configure the surface with the same texture format each time.
	surface_texture_format: TextureFormat,
}

#[allow(clippy::unused_self)] // For accessing resources, we should require the service
impl Wgpu {
	/// Creates the `wgpu` wrapper given the window to create it in, alongside the resource
	pub async fn new(window: Arc<Window>) -> Result<(Self, WgpuSurfaceResource), anyhow::Error> {
		// Create the surface and adapter
		// SAFETY: Due to the window being arced, and we storing it, we ensure the window outlives us and thus the surface
		let (surface, adapter) = unsafe { self::create_surface_and_adapter(&window).await? };

		// Then create the device and it's queue
		let (device, queue) = self::create_device(&adapter).await?;

		// Configure the surface and get the preferred texture format and surface size
		let (surface_texture_format, surface_size) =
			self::configure_window_surface(&window, &surface, &adapter, &device)?;

		tracing::info!("Successfully initialized");

		// Create the service
		let service = Self {
			device,
			queue,
			surface_texture_format,
		};

		// Create the surface resource
		let surface_resource = WgpuSurfaceResource {
			surface,
			size: surface_size,
			_window: window,
		};


		Ok((service, surface_resource))
	}

	/// Returns the wgpu device
	#[must_use]
	pub const fn device(&self) -> &wgpu::Device {
		&self.device
	}

	/// Returns the wgpu queue
	#[must_use]
	pub const fn queue(&self) -> &wgpu::Queue {
		&self.queue
	}

	/// Returns the current surface's size
	///
	/// # Warning
	/// The surface size might change as soon as the surface lock changes,
	/// so you should not keep it afterwards for anything `wgpu` related.
	pub fn surface_size(&self, surface_resource: &WgpuSurfaceResource) -> PhysicalSize<u32> {
		surface_resource.size
	}

	/// Returns the surface texture format
	#[must_use]
	pub const fn surface_texture_format(&self) -> wgpu::TextureFormat {
		self.surface_texture_format
	}

	/// Starts rendering a frame.
	///
	/// Returns the encoder and surface view to render onto
	pub fn start_render(&self, surface_resource: &mut WgpuSurfaceResource) -> Result<FrameRender, anyhow::Error> {
		// And then get the surface texture
		let surface_texture = surface_resource
			.surface
			.get_current_texture()
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
		let encoder = self.device.create_command_encoder(&encoder_descriptor);

		Ok(FrameRender {
			encoder,
			surface_texture,
			surface_view: surface_texture_view,
		})
	}

	/// Checks for and performs a resize
	fn check_resize(&self, input_receiver: &mut InputReceiver, surface_resource: &mut WgpuSurfaceResource) {
		// Note: We only do the last resize as there's no point doing any in-between ones.
		let last_resize = std::iter::from_fn(|| input_receiver.on_resize()).last();
		if let Some(size) = last_resize {
			tracing::info!(?size, "Resizing");
			if size.width > 0 && size.height > 0 {
				// Update our surface
				let config = self::window_surface_configuration(self.surface_texture_format, size);
				surface_resource.surface.configure(&self.device, &config);
				surface_resource.size = size;
			}
		}
	}

	/// Finishes rendering a frame
	pub fn finish_render(
		&self,
		frame: FrameRender,
		surface_resource: &mut WgpuSurfaceResource,
		input_receiver: &mut InputReceiver,
	) {
		// Submit everything to the queue and present the surface's texture
		let _ = self.queue.submit([frame.encoder.finish()]);
		frame.surface_texture.present();

		// Check for resizes
		self.check_resize(input_receiver, surface_resource);
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

	// Create the surface
	// SAFETY: Caller promises the window outlives the surface
	tracing::debug!(?window, "Creating wgpu surface");
	#[deny(unsafe_op_in_unsafe_fn)]
	let surface = unsafe { instance.create_surface(window) };

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

	Ok((surface, adapter))
}

/// Surface resource
// Note: A resource because, once we get the surface texture, calling most other
//       methods, such as `configure` will cause `wgpu` to panic due to the texture
//       being active.
//       For this reason we lock the surface while rendering to ensure no changes happen
//       and no panics can occur.
#[derive(Debug)]
pub struct WgpuSurfaceResource {
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
	size: PhysicalSize<u32>,

	/// Window
	// Note: Our surface must outlive the window, so we make sure of it by arcing it
	_window: Arc<Window>,
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
		present_mode: wgpu::PresentMode::Mailbox,
		alpha_mode:   wgpu::CompositeAlphaMode::Auto,
	}
}
