//! Wgpu renderer

// Imports
use {
	super::WgpuShared,
	winit::{dpi::PhysicalSize, window::Window},
	zutil_app_error::{AppError, Context},
};

/// Wgpu renderer
#[derive(Debug)]
pub struct WgpuRenderer {
	/// Adapter
	adapter: wgpu::Adapter,

	/// Surface
	surface: wgpu::Surface<'static>,

	/// Surface size
	// Note: We keep the size ourselves instead of using the inner
	//       window size because the window resizes asynchronously
	//       from us, so it's possible for the window sizes to be
	//       wrong relative to the surface size.
	//       Wgpu validation code can panic if the size we give it
	//       is invalid (for example, during scissoring), so we *must*
	//       ensure this size is the surface's actual size.
	surface_size: PhysicalSize<u32>,

	/// Surface config
	surface_config: wgpu::SurfaceConfiguration,
}

impl WgpuRenderer {
	pub(super) fn new(
		window: &'static Window,
		surface: wgpu::Surface<'static>,
		adapter: wgpu::Adapter,
		device: &wgpu::Device,
	) -> Result<Self, AppError> {
		// Configure the surface and get the preferred texture format and surface size
		let surface_size = window.inner_size();
		let surface_config = self::configure_window_surface(&surface, &adapter, device, surface_size)
			.context("Unable to configure window surface")?;

		Ok(Self {
			adapter,
			surface,
			surface_size,
			surface_config,
		})
	}

	/// Returns the surface size
	pub fn surface_size(&self) -> PhysicalSize<u32> {
		self.surface_size
	}

	/// Returns the surface config
	pub fn surface_config(&self) -> &wgpu::SurfaceConfiguration {
		&self.surface_config
	}

	/// Starts rendering a frame.
	///
	/// Returns the encoder and surface view to render onto
	// TODO: Ensure it's not called more than once?
	pub fn start_render(&self, shared: &WgpuShared) -> Result<FrameRender, AppError> {
		// And then get the surface texture
		// Note: This can block, so we run it under tokio's block-in-place
		// Note: If the application goes to sleep, this can fail spuriously due to a timeout,
		//       so we keep retrying.
		// TODO: Use an exponential timeout, with a max duration?
		let surface_texture = tokio::task::block_in_place(|| loop {
			match self.surface.get_current_texture() {
				Ok(surface_texture) => break surface_texture,
				Err(err) => tracing::warn!(%err, "Unable to retrieve current texture, retrying"),
			}
		});
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
		})
	}

	/// Performs a resize
	pub fn resize(&mut self, shared: &WgpuShared, size: PhysicalSize<u32>) -> Result<(), AppError> {
		tracing::info!(?size, "Resizing wgpu surface");
		// TODO: Don't ignore resizes to the same size?
		if size.width > 0 && size.height > 0 && size != self.surface_size {
			// Update our surface
			self.surface_config = self::configure_window_surface(&self.surface, &self.adapter, &shared.device, size)
				.context("Unable to configure window surface")?;
			self.surface_size = size;
		}

		Ok(())
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

	/// Surface size
	pub surface_size: PhysicalSize<u32>,
}

impl FrameRender {
	/// Finishes rendering this frame
	pub fn finish(self, shared: &WgpuShared) {
		// Submit everything to the queue and present the surface's texture
		// Note: Although not supposed to, `submit` calls can block, so we wrap it
		//       in a tokio block-in-place
		let _ = tokio::task::block_in_place(|| shared.queue.submit([self.encoder.finish()]));
		self.surface_texture.present();
	}
}

/// Configures the window surface and returns the configuration
fn configure_window_surface(
	surface: &wgpu::Surface<'static>,
	adapter: &wgpu::Adapter,
	device: &wgpu::Device,
	size: PhysicalSize<u32>,
) -> Result<wgpu::SurfaceConfiguration, AppError> {
	// Get the format
	let mut config = surface
		.get_default_config(adapter, size.width, size.height)
		.context("Unable to get surface default config")?;
	tracing::debug!(?config, "Found surface configuration");

	// Set some options
	config.present_mode = wgpu::PresentMode::AutoVsync;
	tracing::debug!(?config, "Updated surface configuration");

	// Then configure it
	surface.configure(device, &config);

	Ok(config)
}
