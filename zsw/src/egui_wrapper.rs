//! Egui wrapper

// Imports
use {
	crate::{
		wgpu_wrapper::{FrameRender, WgpuRenderer, WgpuShared},
		AppError,
	},
	anyhow::Context,
	std::{
		sync::Arc,
		time::{Duration, Instant},
	},
	tokio::sync::mpsc,
	winit::window::Window,
};

/// Egui Renderer
pub struct EguiRenderer {
	/// Render pass
	render_pass: egui_wgpu_backend::RenderPass,
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
		#[allow(clippy::cast_possible_truncation)] // Unfortunately `egui` takes an `f32`
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
	/// Repaint signal
	repaint_signal: Arc<RepaintSignal>,

	/// Last frame time
	frame_time: Option<Duration>,

	/// Platform
	platform: egui_winit_platform::Platform,

	/// Event receiver
	event_rx: mpsc::UnboundedReceiver<winit::event::Event<'static, !>>,
}

impl EguiPainter {
	/// Draws egui
	pub fn draw<E>(
		&mut self,
		window: &Window,
		f: impl FnOnce(&egui::Context, &epi::Frame) -> Result<(), E>,
	) -> Result<egui::FullOutput, E> {
		// If we have events, handle them before drawing
		while let Ok(event) = self.event_rx.try_recv() {
			self.platform.handle_event(&event);
		}

		// Start the frame
		let egui_frame_start = Instant::now();
		self.platform.begin_frame();

		// Create the frame
		let app_output = epi::backend::AppOutput::default();
		#[allow(clippy::cast_possible_truncation)] // Unfortunately `egui` takes an `f32`
		let egui_frame = epi::Frame::new(epi::backend::FrameData {
			info:           epi::IntegrationInfo {
				name:                    "egui",
				web_info:                None,
				cpu_usage:               self.frame_time.as_ref().map(Duration::as_secs_f32),
				native_pixels_per_point: Some(window.scale_factor() as f32),
				prefer_dark_mode:        None,
			},
			output:         app_output,
			repaint_signal: Arc::clone(&self.repaint_signal) as Arc<dyn epi::backend::RepaintSignal>,
		});

		// Then draw using it
		let res = f(&self.platform.context(), &egui_frame);

		// Finally end the frame, retrieve the output and create the paint jobs
		let output = self.platform.end_frame(Some(window));
		self.frame_time = Some(egui_frame_start.elapsed());

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
		#[allow(let_underscore_drop)]
		let _ = self.event_tx.send(event);
	}
}

/// Repaint signal
// Note: We paint egui every frame, so this isn't required currently, but
//       we should take it into consideration eventually.
#[derive(Clone, Copy, Debug)]
struct RepaintSignal;

impl epi::backend::RepaintSignal for RepaintSignal {
	fn request_repaint(&self) {
		static IDX: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
		let idx = IDX.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
		tracing::info!("Repaint {idx}!");
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
	let render_pass =
		egui_wgpu_backend::RenderPass::new(&wgpu_shared.device, wgpu_renderer.surface_texture_format(), 1);

	let (event_tx, event_rx) = mpsc::unbounded_channel();
	(
		EguiRenderer { render_pass },
		EguiPainter {
			repaint_signal: Arc::new(RepaintSignal),
			frame_time: None,
			platform,
			event_rx,
		},
		EguiEventHandler { event_tx },
	)
}
