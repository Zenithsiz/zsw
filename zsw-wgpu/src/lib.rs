//! Wgpu wrapper

// Features
#![feature(must_not_suspend, strict_provenance)]

// Modules
mod renderer;

// Exports
pub use renderer::{FrameRender, WgpuRenderer};

// Imports
use {anyhow::Context, std::sync::Arc, winit::window::Window, zsw_error::AppError};

/// Wgpu shared
#[derive(Debug)]
pub struct WgpuShared {
	/// Device
	pub device: wgpu::Device,

	/// Queue
	pub queue: wgpu::Queue,
}

/// Creates the wgpu service
pub async fn create(window: Arc<Window>) -> Result<(WgpuShared, WgpuRenderer), AppError> {
	// Create the surface and adapter
	// SAFETY: `WgpuRenderer` keeps an `Arc<Window>` to ensure it stays alive for as long as the surface
	let (surface, adapter) = unsafe { self::create_surface_and_adapter(&window) }.await?;

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
	device.on_uncaptured_error(Box::new(|err| {
		tracing::error!("Wgpu error: {err}");
	}));

	Ok((device, queue))
}

/// Creates the surface and adapter
///
/// # Safety
/// The returned surface *must* be dropped before the window.
async unsafe fn create_surface_and_adapter(window: &Window) -> Result<(wgpu::Surface, wgpu::Adapter), AppError> {
	// Get an instance with any backend
	let instance_desc = wgpu::InstanceDescriptor {
		backends:             wgpu::Backends::all(),
		dx12_shader_compiler: wgpu::Dx12Compiler::Dxc {
			dxil_path: None,
			dxc_path:  None,
		},
	};
	// TODO: Just use `?instance_desc` once it implements `Debug`
	tracing::debug!(?instance_desc.backends, ?instance_desc.dx12_shader_compiler, "Requesting wgpu instance");
	let instance = wgpu::Instance::new(instance_desc);
	tracing::debug!(?instance, "Created wgpu instance");

	// Create the surface
	tracing::debug!(?window, "Requesting wgpu surface");
	// SAFETY: Caller promises the window outlives the surface
	let surface = unsafe { instance.create_surface(window) }.context("Unable to request surface")?;
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
