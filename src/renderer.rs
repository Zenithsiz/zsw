//! Renderer

// Modules
mod vertex;

// Exports
pub use vertex::Vertex;

// Imports
use anyhow::Context;
use crossbeam::atomic::AtomicCell;
use parking_lot::Mutex;
use wgpu::TextureFormat;
use winit::{dpi::PhysicalSize, window::Window};

/// Renderer
#[derive(Debug)]
pub struct Renderer {
	/// Surface
	///
	/// We wrap this in a mutex because we can't have the current texture
	/// alive and update it at the same time, so we lock it during rendering.
	surface: Mutex<wgpu::Surface>,

	/// Device
	device: wgpu::Device,

	/// Queue
	queue: wgpu::Queue,

	/// Texture format
	texture_format: TextureFormat,

	/// Size
	size: AtomicCell<PhysicalSize<u32>>,
}

impl Renderer {
	/// Creates the renderer state and starts rendering in another thread
	///
	/// # Errors
	pub async fn new(window: &'static Window) -> Result<Self, anyhow::Error> {
		let size = window.inner_size();

		// Create the surface and adapter
		// SAFETY: `window` is a valid window, and lives for 'static
		let instance = wgpu::Instance::new(wgpu::Backends::all());
		let surface = unsafe { instance.create_surface(window) };
		let adapter_options = wgpu::RequestAdapterOptions {
			power_preference:       wgpu::PowerPreference::default(),
			force_fallback_adapter: false,
			compatible_surface:     Some(&surface),
		};
		let adapter = instance
			.request_adapter(&adapter_options)
			.await
			.context("Unable to request adapter")?;

		// Then create the device and it's queue
		let device_descriptor = wgpu::DeviceDescriptor {
			label:    None,
			features: wgpu::Features::empty(),
			limits:   wgpu::Limits::default(),
		};
		let (device, queue) = adapter
			.request_device(&device_descriptor, None)
			.await
			.context("Unable to request device")?;

		// And setup the config
		let texture_format = surface
			.get_preferred_format(&adapter)
			.context("Unable to query preferred format")?;
		let config = self::surface_configuration(texture_format, size);
		surface.configure(&device, &config);


		Ok(Self {
			surface: Mutex::new(surface),
			device,
			queue,
			texture_format,
			size: AtomicCell::new(size),
		})
	}

	/// Get a reference to the renderer's device.
	pub const fn device(&self) -> &wgpu::Device {
		&self.device
	}

	/// Returns the surface configuration
	pub fn config(&self) -> wgpu::SurfaceConfiguration {
		self::surface_configuration(self.texture_format, self.size.load())
	}

	/// Resizes the underlying surface
	pub fn resize(&self, size: PhysicalSize<u32>) {
		log::info!("Resizing to {size:?}");
		if size.width > 0 && size.height > 0 {
			// Update our size
			self.size.store(size);

			// Update our surface
			let config = self::surface_configuration(self.texture_format, size);
			self.surface.lock().configure(&self.device, &config);
		}
	}

	/// Renders
	pub fn render(&self, f: impl FnOnce(&mut wgpu::CommandEncoder, &wgpu::TextureView)) -> Result<(), anyhow::Error> {
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

		f(&mut encoder, &view);
		self.queue.submit([encoder.finish()]);

		output.present();

		Ok(())
	}
}

/// Returns the surface configuration
const fn surface_configuration(texture_format: TextureFormat, size: PhysicalSize<u32>) -> wgpu::SurfaceConfiguration {
	wgpu::SurfaceConfiguration {
		usage:        wgpu::TextureUsages::RENDER_ATTACHMENT,
		format:       texture_format,
		width:        size.width,
		height:       size.height,
		present_mode: wgpu::PresentMode::Mailbox,
	}
}
