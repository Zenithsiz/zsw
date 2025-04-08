//! Wgpu wrapper

// Features
#![feature(must_not_suspend)]

// Modules
mod renderer;

// Exports
pub use renderer::{FrameRender, WgpuRenderer};

// Imports
use {
	winit::window::Window,
	zutil_app_error::{AppError, Context},
};

/// Wgpu shared
#[derive(Debug)]
pub struct WgpuShared {
	/// Device
	pub device: wgpu::Device,

	/// Queue
	pub queue: wgpu::Queue,
}

/// Creates the wgpu service
pub async fn create(window: &'static Window) -> Result<(WgpuShared, WgpuRenderer), AppError> {
	// Create the surface and adapter
	let (surface, adapter) = self::create_surface_and_adapter(window).await?;

	// Then create the device and it's queue
	let (device, queue) = self::create_device(&adapter).await?;

	// Then create the renderer
	let renderer = WgpuRenderer::new(window, surface, adapter, &device).context("Unable to create renderer")?;

	Ok((WgpuShared { device, queue }, renderer))
}


/// Creates the device
async fn create_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue), AppError> {
	// Request the device without any features
	let device_descriptor = wgpu::DeviceDescriptor {
		label:             Some("[zsw] Device"),
		required_features: wgpu::Features::default(),
		required_limits:   wgpu::Limits::default(),
		memory_hints:      wgpu::MemoryHints::default(),
	};
	tracing::debug!(?device_descriptor, "Requesting wgpu device");
	let (device, queue) = adapter
		.request_device(&device_descriptor, None)
		.await
		.context("Unable to request device")?;

	// Configure the device to not panic on errors
	device.on_uncaptured_error(Box::new(|err| {
		tracing::error!("Wgpu error: {err}");
	}));

	Ok((device, queue))
}

/// Creates the surface and adapter
///
/// # Safety
/// The returned surface *must* be dropped before the window.
async fn create_surface_and_adapter(
	window: &'static Window,
) -> Result<(wgpu::Surface<'static>, wgpu::Adapter), AppError> {
	// Get an instance with any backend
	let instance_desc = wgpu::InstanceDescriptor::from_env_or_default();
	tracing::debug!(?instance_desc, "Requesting wgpu instance");
	let instance = wgpu::Instance::new(&instance_desc);
	tracing::debug!(?instance, "Created wgpu instance");

	// Create the surface
	tracing::debug!(?window, "Requesting wgpu surface");
	let surface = instance.create_surface(window).context("Unable to request surface")?;
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
