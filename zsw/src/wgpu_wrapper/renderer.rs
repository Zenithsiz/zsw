//! Wgpu renderer

// Imports
use {
	super::WgpuShared,
	anyhow::Context,
	std::{marker::PhantomData, sync::Arc},
	winit::{dpi::PhysicalSize, window::Window},
};

/// Wgpu renderer
#[derive(Debug)]
pub struct WgpuRenderer {
	/// Surface
	pub(super) surface: wgpu::Surface,

	/// Surface size
	// Note: We keep the size ourselves instead of using the inner
	//       window size because the window resizes asynchronously
	//       from us, so it's possible for the window sizes to be
	//       wrong relative to the surface size.
	//       Wgpu validation code can panic if the size we give it
	//       is invalid (for example, during scissoring), so we *must*
	//       ensure this size is the surface's actual size.
	pub(super) surface_size: PhysicalSize<u32>,

	/// Window
	pub(super) _window: Arc<Window>,
}

impl WgpuRenderer {
	/// Returns the surface size
	pub fn surface_size(&self) -> PhysicalSize<u32> {
		self.surface_size
	}

	/// Starts rendering a frame.
	///
	/// Returns the encoder and surface view to render onto
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
			_phantom: PhantomData,
		})
	}

	/// Performs a resize
	pub fn resize(&mut self, shared: &WgpuShared, size: PhysicalSize<u32>) {
		tracing::info!(?size, "Resizing wgpu surface");
		if size.width > 0 && size.height > 0 {
			// Update our surface
			let config = super::window_surface_configuration(shared.surface_texture_format, size);
			self.surface.configure(&shared.device, &config);
			self.surface_size = size;
		}
	}
}

/// A frame's rendering
#[derive(Debug)]
pub struct FrameRender<'renderer> {
	/// Encoder
	pub encoder: wgpu::CommandEncoder,

	/// Surface texture
	pub surface_texture: wgpu::SurfaceTexture,

	/// Surface view
	pub surface_view: wgpu::TextureView,

	/// Surface size
	surface_size: PhysicalSize<u32>,

	/// Phantom data to prevent rendering more than a frame at once
	_phantom: PhantomData<&'renderer mut ()>,
}

impl<'renderer> FrameRender<'renderer> {
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
