//! Wgpu
//!
//! See the [`Wgpu`] type for more details

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

		// Start the thread for polling `wgpu`
		thread::Builder::new()
			.name("Wgpu poller".to_owned())
			.spawn(Self::poller_thread(&device))
			.context("Unable to start wgpu poller thread")?;

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
		// Get our surface texture and create a view for it
		let surface = self.surface.lock();
		let surface_texture = surface
			.get_current_texture()
			.context("Unable to retrieve current texture")?;
		let surface_view_descriptor = wgpu::TextureViewDescriptor {
			label: Some("Surface texture view"),
			..wgpu::TextureViewDescriptor::default()
		};
		let surface_texture_view = surface_texture.texture.create_view(&surface_view_descriptor);

		// Then create an encoder for our frame
		let encoder_descriptor = wgpu::CommandEncoderDescriptor {
			label: Some("Render encoder"),
		};
		let mut encoder = self.device.create_command_encoder(&encoder_descriptor);

		// And render using `f`
		f(&mut encoder, &surface_texture_view).context("Unable to render")?;

		// Finally submit everything to the queue and present the texture
		self.queue.submit([encoder.finish()]);
		surface_texture.present();

		Ok(())
	}

	/// Returns the poller thread
	fn poller_thread(device: &Arc<wgpu::Device>) -> impl FnOnce() {
		let device = Arc::clone(device);
		move || {
			log::info!("Starting wgpu poller thread");

			// Poll until the device is gone.
			// TODO: To this in a better way. We currently don't expose the `Arc`,
			//       so the only possible strong counts are 2 and 1, but this may
			//       change in the future and so we'll never actually leave this loop.
			//       Although this isn't super important, since this only happens at exit (for now).
			// TODO: Not sleep here, even with `Wait`, `poll` seems to just return within a few microseconds
			while Arc::strong_count(&device) > 1 {
				device.poll(wgpu::Maintain::Poll);
				thread::sleep(std::time::Duration::from_secs_f32(1.0 / 60.0));
			}

			log::info!("Exiting wgpu poller thread");
		}
	}
}

/// Configures the window surface and returns the preferred texture format
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
