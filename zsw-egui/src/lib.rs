//! Egui wrapper

// Features
#![feature(must_not_suspend, strict_provenance, lint_reasons, never_type)]

// Imports
use {
	anyhow::Context,
	tokio::sync::mpsc,
	tracing as _,
	winit::window::Window,
	zsw_error::AppError,
	zsw_wgpu::{FrameRender, WgpuRenderer, WgpuShared},
};

/// Egui Renderer
pub struct EguiRenderer {
	/// Render pass
	render_pass: egui_wgpu_backend::RenderPass,
}

impl std::fmt::Debug for EguiRenderer {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("EguiRenderer").field("render_pass", &"..").finish()
	}
}

impl EguiRenderer {
	/// Renders egui
	pub fn render_egui(
		&mut self,
		frame: &mut FrameRender,
		window: &winit::window::Window,
		wgpu_shared: &WgpuShared,
		paint_jobs: &[egui::ClippedPrimitive],
		textures_delta: Option<egui::TexturesDelta>,
	) -> Result<(), AppError> {
		// Update textures
		let surface_size = frame.surface_size();
		#[expect(clippy::cast_possible_truncation)] // Unfortunately `egui` takes an `f32`
		let screen_descriptor = egui_wgpu_backend::ScreenDescriptor {
			physical_width:  surface_size.width,
			physical_height: surface_size.height,
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
	platform: egui_winit_platform::Platform,

	/// Event receiver
	event_rx: mpsc::UnboundedReceiver<winit::event::Event<'static, !>>,
}

impl std::fmt::Debug for EguiPainter {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("EguiPainter")
			.field("platform", &"..")
			.field("event_rx", &self.event_rx)
			.finish()
	}
}

impl EguiPainter {
	/// Draws egui
	pub fn draw<E>(
		&mut self,
		window: &Window,
		f: impl FnOnce(&egui::Context) -> Result<(), E>,
	) -> Result<egui::FullOutput, E> {
		// If we have events, handle them before drawing
		while let Ok(event) = self.event_rx.try_recv() {
			self.platform.handle_event(&event);
		}

		// Draw the frame
		self.platform.begin_frame();
		let res = f(&self.platform.context());
		let output = self.platform.end_frame(Some(window));

		res.map(|()| output)
	}

	/// Tessellate the output shapes
	pub fn tessellate_shapes(&mut self, shapes: Vec<egui::epaint::ClippedShape>) -> Vec<egui::ClippedPrimitive> {
		self.platform.context().tessellate(shapes)
	}
}

/// Egui Event handler
#[derive(Debug)]
pub struct EguiEventHandler {
	/// Event sender
	event_tx: mpsc::UnboundedSender<winit::event::Event<'static, !>>,
}

impl EguiEventHandler {
	/// Handles an event
	pub fn handle_event(&mut self, event: winit::event::Event<'static, !>) {
		// Note: We don't care if the event won't be handled
		#[expect(let_underscore_drop)]
		let _ = self.event_tx.send(event);
	}
}

/// Creates the egui service
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

	// Create the egui render pass
	let render_pass = egui_wgpu_backend::RenderPass::new(&wgpu_shared.device, wgpu_renderer.surface_config().format, 1);

	let (event_tx, event_rx) = mpsc::unbounded_channel();
	(
		EguiRenderer { render_pass },
		EguiPainter { platform, event_rx },
		EguiEventHandler { event_tx },
	)
}
