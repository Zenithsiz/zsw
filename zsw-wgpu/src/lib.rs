//! Wgpu wrapper

// Features
#![feature(must_not_suspend)]

// Modules
mod renderer;

// Exports
pub use renderer::{FrameRender, WgpuRenderer};

// Imports
use {
	tokio::sync::OnceCell,
	zutil_app_error::{AppError, Context},
};

/// Wgpu shared
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
}

/// Shared
// TODO: Is it a good idea to make this a global?
//       Realistically, we can only have one per process anyway,
//       so this models that correctly, but it might be bad API.
static SHARED: OnceCell<WgpuShared> = OnceCell::const_new();

/// Gets or creates the shared state
pub async fn get_or_create_shared() -> Result<&'static WgpuShared, AppError> {
	SHARED
		.get_or_try_init(async || {
			let instance = self::create_instance().await.context("Unable to create instance")?;
			let adapter = self::create_adapter(&instance)
				.await
				.context("Unable to create adaptor")?;
			let (device, queue) = self::create_device(&adapter).await.context("Unable to create device")?;

			Ok::<_, AppError>(WgpuShared {
				instance,
				adapter,
				device,
				queue,
			})
		})
		.await
}

/// Creates the device
async fn create_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue), AppError> {
	// Request the device without any features
	let device_descriptor = wgpu::DeviceDescriptor {
		label:             Some("[zsw] Device"),
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

	// Configure the device to not panic on errors
	device.on_uncaptured_error(Box::new(|err| {
		tracing::error!("Wgpu error: {err}");
	}));

	Ok((device, queue))
}

/// Creates the instance
async fn create_instance() -> Result<wgpu::Instance, AppError> {
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
