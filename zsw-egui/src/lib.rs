//! Egui

// Features
#![feature(never_type)]

// Imports
use {
	std::{
		sync::Arc,
		time::{Duration, Instant},
	},
	tokio::sync::mpsc,
	winit::window::Window,
	zsw_wgpu::Wgpu,
};

/// Egui painter
pub struct EguiPainter {
	/// Repaint signal
	repaint_signal: Arc<RepaintSignal>,

	/// Last frame time
	frame_time: Option<Duration>,

	/// Platform
	platform: egui_winit_platform::Platform,

	/// Output sender
	// TODO: Use a custom type instead of a tuple?
	output_tx: mpsc::Sender<(egui::FullOutput, Vec<egui::ClippedPrimitive>)>,

	/// Event receiver
	event_rx: mpsc::UnboundedReceiver<winit::event::Event<'static, !>>,
}

impl EguiPainter {
	/// Draws egui.
	///
	/// Returns `None` if the renderer has quit
	pub async fn draw(&mut self, window: &Window, f: impl FnOnce(&egui::Context, &epi::Frame)) -> Option<()> {
		// If we have events, handle them
		loop {
			match self.event_rx.try_recv() {
				Ok(event) => self.platform.handle_event(&event),
				Err(mpsc::error::TryRecvError::Disconnected) => return None,
				Err(mpsc::error::TryRecvError::Empty) => break,
			}
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
			repaint_signal: self.repaint_signal.clone(),
		});

		// Then draw using it
		f(&self.platform.context(), &egui_frame);

		// Finally end the frame, retrieve the output and create the paint jobs
		let output = self.platform.end_frame(Some(window));
		let paint_jobs = self.platform.context().tessellate(output.shapes.clone());
		self.frame_time = Some(egui_frame_start.elapsed());

		self.output_tx.send((output, paint_jobs)).await.ok()
	}
}

impl std::fmt::Debug for EguiPainter {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("EguiPainter")
			.field("repaint_signal", &self.repaint_signal)
			.field("frame_time", &self.frame_time)
			.field("platform", &"..")
			.field("output_tx", &self.output_tx)
			.field("event_rx", &self.event_rx)
			.finish()
	}
}

/// Egui Renderer
pub struct EguiRenderer {
	/// Render pass
	render_pass: egui_wgpu_backend::RenderPass,

	/// Current output
	cur_output: egui::FullOutput,

	/// Current paint jobs
	cur_paint_jobs: Vec<egui::ClippedPrimitive>,

	/// Output receiver
	output_rx: mpsc::Receiver<(egui::FullOutput, Vec<egui::ClippedPrimitive>)>,
}

impl EguiRenderer {
	/// Returns the render pass and output.
	///
	/// Returns `None` if either the painter or event handler quit
	#[must_use]
	pub fn render_pass_with_output(
		&mut self,
	) -> Option<(
		&mut egui_wgpu_backend::RenderPass,
		&egui::FullOutput,
		&[egui::ClippedPrimitive],
	)> {
		// If we have a new output, update them
		// TODO: Not panic here when the painter quit
		match self.output_rx.try_recv() {
			Ok((output, paint_jobs)) => {
				self.cur_output = output;
				self.cur_paint_jobs = paint_jobs;
			},
			Err(mpsc::error::TryRecvError::Disconnected) => return None,
			Err(mpsc::error::TryRecvError::Empty) => (),
		};

		Some((&mut self.render_pass, &self.cur_output, &self.cur_paint_jobs))
	}
}

impl std::fmt::Debug for EguiRenderer {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("EguiRenderer")
			.field("render_pass", &"..")
			.field("output", &"..")
			.field("output_rx", &self.output_rx)
			.finish()
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

/// Creates the egui service
pub fn create(window: &Window, wgpu: &Wgpu) -> (EguiRenderer, EguiPainter, EguiEventHandler) {
	// Create the egui platform
	// TODO: Check if it's fine to use the window size here instead of the
	//       wgpu surface size
	let window_size = window.inner_size();
	let platform = egui_winit_platform::Platform::new(egui_winit_platform::PlatformDescriptor {
		physical_width:   window_size.width,
		physical_height:  window_size.height,
		scale_factor:     window.scale_factor(),
		font_definitions: egui::FontDefinitions::default(),
		style:            egui::Style::default(),
	});

	// Create the egui render pass
	let render_pass = egui_wgpu_backend::RenderPass::new(wgpu.device(), wgpu.surface_texture_format(), 1);

	// Create the egui repaint signal
	let repaint_signal = Arc::new(RepaintSignal);

	// TODO: We can't use a 1-size channel for some reason, the `output_tx` gets stuck
	//       sending the 2nd output and never continues.
	let (output_tx, output_rx) = mpsc::channel(2);
	let (event_tx, event_rx) = mpsc::unbounded_channel();

	(
		EguiRenderer {
			render_pass,
			cur_output: egui::FullOutput::default(),
			cur_paint_jobs: vec![],
			output_rx,
		},
		EguiPainter {
			repaint_signal,
			frame_time: None,
			platform,
			output_tx,
			event_rx,
		},
		EguiEventHandler { event_tx },
	)
}

/// Repaint signal
// Note: We paint egui every frame, so this isn't required currently, but
//       we should take it into consideration eventually.
#[derive(Clone, Copy, Debug)]
struct RepaintSignal;

impl epi::backend::RepaintSignal for RepaintSignal {
	fn request_repaint(&self) {}
}
