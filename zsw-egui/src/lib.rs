//! Egui

// Features
#![feature(never_type, let_chains)]

// Imports
use {
	anyhow::Context,
	crossbeam::atomic::AtomicCell,
	futures::{channel::mpsc, SinkExt},
	std::{
		sync::Arc,
		time::{Duration, Instant},
	},
	winit::window::Window,
	zsw_wgpu::Wgpu,
};


/// All egui state
pub struct Egui {
	/// Repaint signal
	repaint_signal: Arc<RepaintSignal>,

	/// Last frame time
	frame_time: AtomicCell<Option<Duration>>,
}

impl std::fmt::Debug for Egui {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Egui")
			.field("repaint_signal", &self.repaint_signal)
			.field("frame_time", &self.frame_time)
			.finish()
	}
}

#[allow(clippy::unused_self)] // For accessing resources, we should require the service
impl Egui {
	/// Creates the egui state
	pub fn new(
		window: &Window,
		wgpu: &Wgpu,
	) -> Result<(Self, EguiPlatformResource, EguiRenderPassResource, EguiPainterResource), anyhow::Error> {
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

		// Create the service
		let service = Self {
			repaint_signal,
			frame_time: AtomicCell::new(None),
		};

		// Create the resources
		// Note: By using a 0-size channel we achieve the least latency
		let (paint_jobs_tx, paint_jobs_rx) = mpsc::channel(0);
		let platform_resource = EguiPlatformResource { platform };
		let render_pass_resource = EguiRenderPassResource {
			render_pass,
			paint_jobs: vec![],
			paint_jobs_rx,
		};
		let painter_resource = EguiPainterResource { paint_jobs_tx };

		Ok((service, platform_resource, render_pass_resource, painter_resource))
	}

	/// Draws egui
	pub fn draw(
		&self,
		window: &Window,
		platform_resource: &mut EguiPlatformResource,
		f: impl FnOnce(&egui::CtxRef, &epi::Frame) -> Result<(), anyhow::Error>,
	) -> Result<Vec<egui::ClippedMesh>, anyhow::Error> {
		// Start the frame
		let egui_frame_start = Instant::now();
		platform_resource.platform.begin_frame();

		// Create the frame
		let app_output = epi::backend::AppOutput::default();
		#[allow(clippy::cast_possible_truncation)] // Unfortunately `egui` takes an `f32`
		let egui_frame = epi::Frame::new(epi::backend::FrameData {
			info:           epi::IntegrationInfo {
				name:                    "egui",
				web_info:                None,
				cpu_usage:               self.frame_time.load().as_ref().map(Duration::as_secs_f32),
				native_pixels_per_point: Some(window.scale_factor() as f32),
				prefer_dark_mode:        None,
			},
			output:         app_output,
			repaint_signal: self.repaint_signal.clone(),
		});

		// Then draw using it
		f(&platform_resource.platform.context(), &egui_frame).context("Unable to draw")?;

		// Finally end the frame and retrieve the paint jobs
		let (_output, paint_commands) = platform_resource.platform.end_frame(Some(window));
		let paint_jobs = platform_resource.platform.context().tessellate(paint_commands);
		self.frame_time.store(Some(egui_frame_start.elapsed()));

		Ok(paint_jobs)
	}

	/// Handles an event
	pub fn handle_event(&self, platform_resource: &mut EguiPlatformResource, event: &winit::event::Event<!>) {
		platform_resource.platform.handle_event(event);
	}

	/// Returns the font image
	pub fn font_image(&self, platform_resource: &EguiPlatformResource) -> Arc<egui::FontImage> {
		platform_resource.platform.context().font_image()
	}

	/// Returns the render pass and paint jobs
	pub fn render_pass_with_paint_jobs<'a>(
		&self,
		render_pass_resource: &'a mut EguiRenderPassResource,
	) -> (&'a mut egui_wgpu_backend::RenderPass, &'a [egui::ClippedMesh]) {
		// If we have any new paint jobs, update them
		// TODO: Not panic here when the painter quit
		if let Ok(paint_jobs) = render_pass_resource
			.paint_jobs_rx
			.try_next()
			.transpose()
			.expect("Egui painter quit")
		{
			render_pass_resource.paint_jobs = paint_jobs;
		}

		(&mut render_pass_resource.render_pass, &render_pass_resource.paint_jobs)
	}

	/// Updates the paint jobs
	///
	/// Returns `Err` if they haven't been fetched yet
	pub async fn update_paint_jobs(
		&self,
		painter_resource: &mut EguiPainterResource,
		paint_jobs: Vec<egui::ClippedMesh>,
	) {
		// TDO: Not panic
		painter_resource
			.paint_jobs_tx
			.send(paint_jobs)
			.await
			.expect("Egui renderer quit");
	}
}

/// Platform resource
pub struct EguiPlatformResource {
	/// Platform
	platform: egui_winit_platform::Platform,
}

impl std::fmt::Debug for EguiPlatformResource {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("EguiPlatformResource").field("platform", &"..").finish()
	}
}

/// Render pass resource
pub struct EguiRenderPassResource {
	/// Render pass
	render_pass: egui_wgpu_backend::RenderPass,

	/// Current paint jobs
	paint_jobs: Vec<egui::ClippedMesh>,

	/// Paint jobs receiver
	paint_jobs_rx: mpsc::Receiver<Vec<egui::ClippedMesh>>,
}

impl std::fmt::Debug for EguiRenderPassResource {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("EguiRenderPassResource")
			.field("render_pass", &"..")
			.finish()
	}
}

/// Painter resource
#[derive(Debug)]
pub struct EguiPainterResource {
	/// Paint jobs sender
	paint_jobs_tx: mpsc::Sender<Vec<egui::ClippedMesh>>,
}

/// Repaint signal
// Note: We paint egui every frame, so this isn't required currently, but
//       we should take it into consideration eventually.
#[derive(Clone, Copy, Debug)]
pub struct RepaintSignal;

impl epi::backend::RepaintSignal for RepaintSignal {
	fn request_repaint(&self) {}
}
