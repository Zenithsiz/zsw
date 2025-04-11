//! Egui wrapper

// Features
#![feature(must_not_suspend, never_type)]

// Imports
use {
	egui::epaint,
	std::{fmt, sync::Arc},
	tokio::sync::Mutex,
	tracing as _,
	winit::window::Window,
	zsw_wgpu::{FrameRender, WgpuRenderer, WgpuShared},
	zutil_app_error::{AppError, Context},
};

/// Egui Renderer
pub struct EguiRenderer {
	/// Render pass
	render_pass: egui_wgpu_backend::RenderPass,
}

impl fmt::Debug for EguiRenderer {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("EguiRenderer").field("render_pass", &"..").finish()
	}
}

impl EguiRenderer {
	/// Renders egui
	pub fn render_egui(
		&mut self,
		frame: &mut FrameRender,
		window: &Window,
		wgpu_shared: &WgpuShared,
		paint_jobs: &[egui::ClippedPrimitive],
		textures_delta: Option<egui::TexturesDelta>,
	) -> Result<(), AppError> {
		// Update textures
		#[expect(clippy::cast_possible_truncation)] // Unfortunately `egui` takes an `f32`
		let screen_descriptor = egui_wgpu_backend::ScreenDescriptor {
			physical_width:  frame.surface_size.width,
			physical_height: frame.surface_size.height,
			scale_factor:    window.scale_factor() as f32,
		};

		// If we have any textures delta, update them
		if let Some(textures_delta) = textures_delta.as_ref() {
			self.render_pass
				.add_textures(&wgpu_shared.device, &wgpu_shared.queue, textures_delta)
				.context("Unable to update textures")?;
		}

		// Update buffers
		self.render_pass
			.update_buffers(&wgpu_shared.device, &wgpu_shared.queue, paint_jobs, &screen_descriptor);

		// Record all render passes.
		self.render_pass
			.execute(
				&mut frame.encoder,
				&frame.surface_view,
				paint_jobs,
				&screen_descriptor,
				None,
			)
			.context("Unable to render egui")?;

		// Then remove any unneeded textures at the end, if we have any
		if let Some(textures_delta) = textures_delta {
			self.render_pass
				.remove_textures(textures_delta)
				.context("Unable to update textures")?;
		}

		Ok(())
	}
}

/// Egui drawer
pub struct EguiPainter {
	/// Platform
	platform: Arc<Mutex<egui_winit_platform::Platform>>,
}

impl fmt::Debug for EguiPainter {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("EguiPainter").field("platform", &"..").finish()
	}
}

impl EguiPainter {
	/// Draws egui
	pub async fn draw<E>(
		&self,
		window: &Window,
		f: impl AsyncFnOnce(&egui::Context) -> Result<(), E>,
	) -> Result<egui::FullOutput, E> {
		let mut platform = self.platform.lock().await;

		// Draw the frame
		platform.begin_pass();
		let res = f(&platform.context()).await;
		let output = platform.end_pass(Some(window));

		res.map(|()| output)
	}

	/// Tessellate the output shapes
	pub async fn tessellate_shapes(
		&self,
		shapes: Vec<epaint::ClippedShape>,
		pixels_per_point: f32,
	) -> Vec<egui::ClippedPrimitive> {
		self.platform
			.lock()
			.await
			.context()
			.tessellate(shapes, pixels_per_point)
	}
}

/// Egui Event handler
pub struct EguiEventHandler {
	/// Platform
	platform: Arc<Mutex<egui_winit_platform::Platform>>,
}

impl fmt::Debug for EguiEventHandler {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("EguiEventHandler").field("platform", &"..").finish()
	}
}

impl EguiEventHandler {
	/// Handles an event
	pub async fn handle_event(&self, event: &winit::event::WindowEvent) {
		self.platform.lock().await.handle_event(event);
	}
}

/// Creates the egui service
#[must_use]
pub fn create(
	window: &Window,
	wgpu_renderer: &WgpuRenderer,
	wgpu_shared: &WgpuShared,
) -> (EguiRenderer, EguiPainter, EguiEventHandler) {
	// Create the egui platform
	let surface_size = wgpu_renderer.surface_size();
	let platform = egui_winit_platform::Platform::new(egui_winit_platform::PlatformDescriptor {
		physical_width:   surface_size.width,
		physical_height:  surface_size.height,
		scale_factor:     window.scale_factor(),
		font_definitions: egui::FontDefinitions::default(),
		style:            egui::Style::default(),
	});
	let platform = Arc::new(Mutex::new(platform));

	// Create the egui render pass
	let render_pass = egui_wgpu_backend::RenderPass::new(&wgpu_shared.device, wgpu_renderer.surface_config().format, 1);

	(
		EguiRenderer { render_pass },
		EguiPainter {
			platform: Arc::clone(&platform),
		},
		EguiEventHandler { platform },
	)
}
