//! Wgpu renderer

// Imports
use {
	super::WgpuShared,
	anyhow::Context,
	std::sync::Arc,
	winit::{dpi::PhysicalSize, window::Window},
};

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

	/// Surface texture format
	surface_texture_format: wgpu::TextureFormat,
}

impl WgpuRenderer {
	pub(super) fn new(
		window: Arc<Window>,
		surface: wgpu::Surface,
		adapter: &wgpu::Adapter,
		device: &wgpu::Device,
	) -> Result<Self, anyhow::Error> {
		// Configure the surface and get the preferred texture format and surface size
		let (surface_texture_format, surface_size) = self::configure_window_surface(&window, &surface, adapter, device)
			.context("Unable to configure window surface")?;

		Ok(Self {
			surface,
			surface_size,
			_window: window,
			surface_texture_format,
		})
	}

	/// Returns the surface size
	pub fn surface_size(&self) -> PhysicalSize<u32> {
		self.surface_size
	}

	/// Returns the surface texture format
	pub fn surface_texture_format(&self) -> wgpu::TextureFormat {
		self.surface_texture_format
	}

	/// Starts rendering a frame.
	///
	/// Returns the encoder and surface view to render onto
	// TODO: Ensure it's not called more than once?
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
		})
	}

	/// Performs a resize
	pub fn resize(&mut self, shared: &WgpuShared, size: PhysicalSize<u32>) {
		tracing::info!(?size, "Resizing wgpu surface");
		if size.width > 0 && size.height > 0 {
			// Update our surface
			let config = self::window_surface_configuration(self.surface_texture_format, size);
			self.surface.configure(&shared.device, &config);
			self.surface_size = size;
		}
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
	surface_size: PhysicalSize<u32>,
}

impl FrameRender {
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
) -> Result<(wgpu::TextureFormat, PhysicalSize<u32>), anyhow::Error> {
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

/// Returns the window surface configuration
const fn window_surface_configuration(
	surface_texture_format: wgpu::TextureFormat,
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
