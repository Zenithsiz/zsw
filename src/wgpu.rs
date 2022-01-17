//! Wgpu
//!
//! See the [`Wgpu`] type for more details

// Modules
//mod vertex;

// Exports
//pub use vertex::Vertex;

// Imports
use anyhow::Context;
use parking_lot::Mutex;
use std::{sync::Arc, thread};
use wgpu::TextureFormat;
use winit::{dpi::PhysicalSize, window::Window};

/// Wgpu renderer
///
/// Responsible for interfacing with `wgpu`.
#[derive(Debug)]
pub struct Wgpu {
	/// Device
	device: Arc<wgpu::Device>,

	/// Queue
	queue: wgpu::Queue,

	/// Surface
	// Note: Wrapped in a mutex because `wgpu` panics when we try to update the texture
	//       (during resizing) when we're currently writing to it mid-frame, so we get
	//       an exclusive lock on it.
	surface: Mutex<wgpu::Surface>,

	/// Preferred texture format
	texture_format: TextureFormat,
	/*
	/// Geometry indices
	geometry_indices: wgpu::Buffer,

	/// Render pipeline for all geometry
	geometry_render_pipeline: wgpu::RenderPipeline,
	*/
}

impl Wgpu {
	/// Creates the renderer state and starts rendering in another thread
	///
	/// # Errors
	pub async fn new(window: &'static Window) -> Result<Self, anyhow::Error> {
		// Create the surface and adapter
		let (surface, adapter) = self::create_surface_and_adaptor(window).await?;

		// Then create the device and it's queue
		let (device, queue) = self::create_device(&adapter).await?;
		let device = Arc::new(device);

		// Configure the surface and get the preferred texture format
		let texture_format = self::configure_window_surface(window, &surface, &adapter, &device)?;

		// Create the geometry indices
		//let geometry_indices = self::create_geometry_indices(&device);

		// Start the thread for polling `wgpu`
		{
			let device = Arc::clone(&device);
			thread::Builder::new()
				.name("Wgpu poller".to_owned())
				.spawn(move || loop {
					device.poll(wgpu::Maintain::Wait);
				})
				.context("Unable to start wgpu poller thread")?;
		}

		Ok(Self {
			surface: Mutex::new(surface),
			device,
			queue,
			texture_format,
		})
	}

	/// Returns the wgpu device
	pub fn device(&self) -> &wgpu::Device {
		&self.device
	}

	/// Returns the wgpu queue
	pub const fn queue(&self) -> &wgpu::Queue {
		&self.queue
	}

	/// Returns the preferred texture format
	pub const fn texture_format(&self) -> wgpu::TextureFormat {
		self.texture_format
	}

	/// Resizes the underlying surface
	pub fn resize(&self, size: PhysicalSize<u32>) {
		log::info!("Resizing to {size:?}");
		if size.width > 0 && size.height > 0 {
			// Update our surface
			let config = self::window_surface_configuration(self.texture_format, size);
			self.surface.lock().configure(&self.device, &config);
		}
	}

	/// Renders
	pub fn render(
		&self, f: impl FnOnce(&mut wgpu::CommandEncoder, &wgpu::TextureView) -> Result<(), anyhow::Error>,
	) -> Result<(), anyhow::Error> {
		let surface = self.surface.lock();
		let output = surface
			.get_current_texture()
			.context("Unable to retrieve current texture")?;
		let view_descriptor = wgpu::TextureViewDescriptor::default();
		let view = output.texture.create_view(&view_descriptor);

		let encoder_descriptor = wgpu::CommandEncoderDescriptor {
			label: Some("Render encoder"),
		};
		let mut encoder = self.device.create_command_encoder(&encoder_descriptor);

		f(&mut encoder, &view).context("Unable to render")?;
		self.queue.submit([encoder.finish()]);

		output.present();

		Ok(())
	}
}

/// Configures the window surface
fn configure_window_surface(
	window: &Window, surface: &wgpu::Surface, adapter: &wgpu::Adapter, device: &wgpu::Device,
) -> Result<TextureFormat, anyhow::Error> {
	// Get the format
	let texture_format = surface
		.get_preferred_format(adapter)
		.context("Unable to query preferred format")?;

	// Then configure it
	let config = self::window_surface_configuration(texture_format, window.inner_size());
	surface.configure(device, &config);

	Ok(texture_format)
}

/// Creates the device
async fn create_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue), anyhow::Error> {
	// Request the device without any features
	let device_descriptor = wgpu::DeviceDescriptor {
		label:    None,
		features: wgpu::Features::empty(),
		limits:   wgpu::Limits::default(),
	};
	let (device, queue) = adapter
		.request_device(&device_descriptor, None)
		.await
		.context("Unable to request device")?;

	Ok((device, queue))
}

/// Creates the surface and adaptor
async fn create_surface_and_adaptor(window: &'static Window) -> Result<(wgpu::Surface, wgpu::Adapter), anyhow::Error> {
	// Get an instance with any backend
	let instance = wgpu::Instance::new(wgpu::Backends::all());

	// Create the surface
	// SAFETY: `window` has a `'static` lifetime
	let surface = unsafe { instance.create_surface(window) };
	let adapter_options = wgpu::RequestAdapterOptions {
		power_preference:       wgpu::PowerPreference::default(),
		force_fallback_adapter: false,
		compatible_surface:     Some(&surface),
	};

	// Then request the adapter
	let adapter = instance
		.request_adapter(&adapter_options)
		.await
		.context("Unable to request adapter")?;

	Ok((surface, adapter))
}

/// Returns the window surface configuration
const fn window_surface_configuration(
	texture_format: TextureFormat, size: PhysicalSize<u32>,
) -> wgpu::SurfaceConfiguration {
	wgpu::SurfaceConfiguration {
		usage:        wgpu::TextureUsages::RENDER_ATTACHMENT,
		format:       texture_format,
		width:        size.width,
		height:       size.height,
		present_mode: wgpu::PresentMode::Mailbox,
	}
}
