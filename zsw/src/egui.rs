//! Egui

// Imports
use {
	crate::{
		util::{extse::ParkingLotMutexSe, MightBlock},
		Wgpu,
	},
	anyhow::Context,
	crossbeam::atomic::AtomicCell,
	parking_lot::Mutex,
	std::{
		sync::Arc,
		time::{Duration, Instant},
	},
	winit::window::Window,
	zsw_side_effect_macros::side_effect,
};


/// All egui state
pub struct Egui {
	/// Platform
	// DEADLOCK: We ensure this lock can't deadlock by not blocking
	//           while locked.
	platform: Mutex<egui_winit_platform::Platform>,

	/// Render pass
	// DEADLOCK: We ensure this lock can't deadlock by not blocking
	//           while locked.
	render_pass: Mutex<egui_wgpu_backend::RenderPass>,

	/// Repaint signal
	repaint_signal: Arc<RepaintSignal>,

	/// Last frame time
	frame_time: AtomicCell<Option<Duration>>,
}

impl std::fmt::Debug for Egui {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Egui")
			.field("platform", &"..")
			.field("render_pass", &"..")
			.field("repaint_signal", &"..")
			.field("frame_time", &self.frame_time)
			.finish()
	}
}

impl Egui {
	/// Creates the egui state
	pub fn new(window: &Window, wgpu: &Wgpu) -> Result<Self, anyhow::Error> {
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

		Ok(Self {
			platform: Mutex::new(platform),
			render_pass: Mutex::new(render_pass),
			repaint_signal,
			frame_time: AtomicCell::new(None),
		})
	}

	/// Draws egui
	///
	/// If called simultaneously from multiple threads,
	/// frame times may be wrong.
	pub fn draw(
		&self,
		window: &Window,
		f: impl FnOnce(&egui::CtxRef, &epi::Frame) -> Result<(), anyhow::Error>,
	) -> Result<Vec<egui::ClippedMesh>, anyhow::Error> {
		// Start the frame
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked.
		// Note: We must keep the platform locked until the call to retrieve all the paint jobs,
		//       in order to ensure the main thread can't process events mid-drawing.
		let mut egui_platform = self.platform.lock_se().allow::<MightBlock>();
		let egui_frame_start = Instant::now();
		egui_platform.begin_frame();

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
		f(&egui_platform.context(), &egui_frame).context("Unable to draw")?;

		// Finally end the frame and retrieve the paint jobs
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked.
		let (_output, paint_commands) = egui_platform.end_frame(Some(window));
		let paint_jobs = egui_platform.context().tessellate(paint_commands);
		self.frame_time.store(Some(egui_frame_start.elapsed()));

		Ok(paint_jobs)
	}

	/// Handles an event
	pub fn handle_event(&self, event: &winit::event::Event<!>) {
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked.
		self.platform.lock_se().allow::<MightBlock>().handle_event(event);
	}

	/// Returns the font image
	pub fn font_image(&self) -> Arc<egui::FontImage> {
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked.
		self.platform.lock_se().allow::<MightBlock>().context().font_image()
	}

	/// Performs a render pass
	///
	/// # Blocking
	/// `f` must not block.
	#[side_effect(MightBlock)]
	pub fn do_render_pass<T>(&self, f: impl FnOnce(&mut egui_wgpu_backend::RenderPass) -> T) -> T {
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked.
		//           Caller ensures to not block.
		let mut render_pass = self.render_pass.lock_se().allow::<MightBlock>();
		f(&mut *render_pass)
	}
}

/// Repaint signal
// Note: We paint egui every frame, so this isn't required currently, but
//       we should take it into consideration eventually.
pub struct RepaintSignal;

impl epi::backend::RepaintSignal for RepaintSignal {
	fn request_repaint(&self) {}
}
