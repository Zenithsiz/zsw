//! Wgpu wrapper

// Modules
mod renderer;

// Exports
pub use renderer::{FrameRender, WgpuRenderer};

// Imports
use {
	anyhow::Context,
	std::sync::Arc,
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
	// TODO: Move to renderer and keep an `Arc<AtomicCell<TextureFormat>>` or similar here instead
	pub surface_texture_format: TextureFormat,
}

/// Creates the wgpu service
pub async fn create(window: Arc<Window>) -> Result<(WgpuShared, WgpuRenderer), anyhow::Error> {
	// Create the surface and adapter
	// SAFETY: We keep a reference to the window, to ensure it outlives the surface
	let (surface, adapter) = unsafe { self::create_surface_and_adapter(&window).await? };

	// Then create the device and it's queue
	let (device, queue) = self::create_device(&adapter).await?;

	// Configure the surface and get the preferred texture format and surface size
	let (surface_texture_format, surface_size) = self::configure_window_surface(&window, &surface, &adapter, &device)?;

	Ok((
		WgpuShared {
			device,
			queue,
			surface_texture_format,
		},
		WgpuRenderer {
			surface,
			surface_size,
			_window: window,
		},
	))
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
