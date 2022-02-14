//! Egui

// Features
#![feature(never_type)]
// Lints
#![warn(
	clippy::pedantic,
	clippy::nursery,
	missing_copy_implementations,
	missing_debug_implementations,
	noop_method_call,
	unused_results
)]
#![deny(
	// We want to annotate unsafe inside unsafe fns
	unsafe_op_in_unsafe_fn,
	// We muse use `expect` instead
	clippy::unwrap_used
)]
#![allow(
	// Style
	clippy::implicit_return,
	clippy::multiple_inherent_impl,
	clippy::pattern_type_mismatch,
	// `match` reads easier than `if / else`
	clippy::match_bool,
	clippy::single_match_else,
	//clippy::single_match,
	clippy::self_named_module_files,
	clippy::items_after_statements,
	clippy::module_name_repetitions,
	// Performance
	clippy::suboptimal_flops, // We prefer readability
	// Some functions might return an error in the future
	clippy::unnecessary_wraps,
	// Due to working with windows and rendering, which use `u32` / `f32` liberally
	// and interchangeably, we can't do much aside from casting and accepting possible
	// losses, although most will be lossless, since we deal with window sizes and the
	// such, which will fit within a `f32` losslessly.
	clippy::cast_precision_loss,
	clippy::cast_possible_truncation,
	// We use proper error types when it matters what errors can be returned, else,
	// such as when using `anyhow`, we just assume the caller won't check *what* error
	// happened and instead just bubbles it up
	clippy::missing_errors_doc,
	// Too many false positives and not too important
	clippy::missing_const_for_fn,
	// This is a binary crate, so we don't expose any API
	rustdoc::private_intra_doc_links,
)]

// Imports
use {
	anyhow::Context,
	crossbeam::atomic::AtomicCell,
	parking_lot::Mutex,
	std::{
		sync::Arc,
		time::{Duration, Instant},
	},
	winit::window::Window,
	zsw_side_effect_macros::side_effect,
	zsw_util::{extse::ParkingLotMutexSe, MightBlock, MightLock},
	zsw_wgpu::Wgpu,
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

	/// Lock source
	lock_source: LockSource,
}

impl std::fmt::Debug for Egui {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Egui")
			.field("platform", &"..")
			.field("render_pass", &"..")
			.field("repaint_signal", &"..")
			.field("frame_time", &self.frame_time)
			.field("lock_source", &self.lock_source)
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
			lock_source: LockSource,
		})
	}

	/// Creates a platform lock
	///
	/// # Blocking
	/// Will block until any existing platform locks are dropped
	#[side_effect(MightLock<PlatformLock>)]
	pub fn lock_platform(&self) -> PlatformLock {
		// DEADLOCK: Caller is responsible to ensure we don't deadlock
		//           We don't lock it outside of this method
		let guard = self.platform.lock_se().allow::<MightBlock>();
		PlatformLock::new(guard, &self.lock_source)
	}

	/// Creates a render pas lock
	///
	/// # Blocking
	/// Will block until any existing render pass locks are dropped
	#[side_effect(MightLock<RenderPassLock>)]
	pub fn lock_render_pass(&self) -> RenderPassLock {
		// DEADLOCK: Caller is responsible to ensure we don't deadlock
		//           We don't lock it outside of this method
		let guard = self.render_pass.lock_se().allow::<MightBlock>();
		RenderPassLock::new(guard, &self.lock_source)
	}

	/// Draws egui
	///
	/// If called simultaneously from multiple threads,
	/// frame times may be wrong.
	pub fn draw(
		&self,
		window: &Window,
		platform_lock: &mut PlatformLock,
		f: impl FnOnce(&egui::CtxRef, &epi::Frame) -> Result<(), anyhow::Error>,
	) -> Result<Vec<egui::ClippedMesh>, anyhow::Error> {
		// Start the frame
		let egui_platform = platform_lock.get_mut(&self.lock_source);
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
	pub fn handle_event(&self, platform_lock: &mut PlatformLock, event: &winit::event::Event<!>) {
		platform_lock.get_mut(&self.lock_source).handle_event(event);
	}

	/// Returns the font image
	pub fn font_image(&self, platform_lock: &PlatformLock) -> Arc<egui::FontImage> {
		platform_lock.get(&self.lock_source).context().font_image()
	}

	/// Performs a render pass
	pub fn do_render_pass<T>(
		&self,
		render_pass_lock: &mut RenderPassLock,
		f: impl FnOnce(&mut egui_wgpu_backend::RenderPass) -> T,
	) -> T {
		let render_pass = render_pass_lock.get_mut(&self.lock_source);
		f(render_pass)
	}
}

/// Source for all locks
// Note: This is to ensure user can't create the locks themselves
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct LockSource;

/// Platform lock
pub type PlatformLock<'a> = zsw_util::Lock<'a, egui_winit_platform::Platform, LockSource>;

/// Render pass lock
pub type RenderPassLock<'a> = zsw_util::Lock<'a, egui_wgpu_backend::RenderPass, LockSource>;

/// Repaint signal
// Note: We paint egui every frame, so this isn't required currently, but
//       we should take it into consideration eventually.
#[derive(Clone, Copy, Debug)]
pub struct RepaintSignal;

impl epi::backend::RepaintSignal for RepaintSignal {
	fn request_repaint(&self) {}
}
