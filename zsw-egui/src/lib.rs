//! Egui

// Features
#![feature(never_type, let_chains)]
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
	// This is too prevalent on generic functions, which we don't want to ALWAYS be `Send`
	clippy::future_not_send,
)]

// Imports
use {
	anyhow::Context,
	crossbeam::atomic::AtomicCell,
	std::{
		sync::Arc,
		task::Waker,
		time::{Duration, Instant},
	},
	winit::window::Window,
	zsw_util::FetchUpdate,
	zsw_wgpu::Wgpu,
};


/// All egui state
pub struct Egui {
	/// Repaint signal
	repaint_signal: Arc<RepaintSignal>,

	/// Last frame time
	frame_time: AtomicCell<Option<Duration>>,

	/// Paint jobs waker
	paint_jobs_waker: AtomicCell<Option<Waker>>,
}

impl std::fmt::Debug for Egui {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Egui")
			.field("repaint_signal", &self.repaint_signal)
			.field("frame_time", &self.frame_time)
			.field("paint_jobs_waker", &"..")
			.finish()
	}
}

#[allow(clippy::unused_self)] // For accessing resources, we should require the service
impl Egui {
	/// Creates the egui state
	pub fn new(
		window: &Window,
		wgpu: &Wgpu,
	) -> Result<
		(
			Self,
			EguiPlatformResource,
			EguiRenderPassResource,
			EguiPaintJobsResource,
		),
		anyhow::Error,
	> {
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
			paint_jobs_waker: AtomicCell::new(None),
		};

		// Create the resources
		let platform_resource = EguiPlatformResource { platform };
		let render_pass_resource = EguiRenderPassResource { render_pass };
		let paint_jobs_resource = EguiPaintJobsResource {
			paint_jobs: FetchUpdate::new(vec![]),
		};


		Ok((service, platform_resource, render_pass_resource, paint_jobs_resource))
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

	/// Returns the current paint jobs
	pub fn paint_jobs<'a>(&self, paint_jobs_resource: &'a mut EguiPaintJobsResource) -> &'a [egui::ClippedMesh] {
		// Get the paint jobs
		let paint_jobs = paint_jobs_resource.paint_jobs.fetch();

		// If we have a waker, wake them
		if let Some(waker) = self.paint_jobs_waker.take() {
			waker.wake();
		}

		paint_jobs
	}

	/// Returns the render pass
	pub fn render_pass<'a>(
		&self,
		render_pass_resource: &'a mut EguiRenderPassResource,
	) -> &'a mut egui_wgpu_backend::RenderPass {
		&mut render_pass_resource.render_pass
	}

	/// Updates the paint jobs
	///
	/// Returns `Err` if they haven't been fetched yet
	pub async fn update_paint_jobs(
		&self,
		paint_jobs_resource: &mut EguiPaintJobsResource,
		paint_jobs: Vec<egui::ClippedMesh>,
	) -> Result<(), Vec<egui::ClippedMesh>> {
		paint_jobs_resource.paint_jobs.update(paint_jobs)
	}

	/// Registers a waker to be woken up by the paint jobs being fetched
	pub fn set_paint_jobs_waker(&self, paint_jobs_resource: &EguiPaintJobsResource, waker: Waker) {
		// Set the waker
		self.paint_jobs_waker.store(Some(waker));

		// If the paint jobs were fetched in the meantime without waking, wake up
		if paint_jobs_resource.paint_jobs.is_seen() && let Some(waker) = self.paint_jobs_waker.take() {
			waker.wake();
		}
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
	render_pass: egui_wgpu_backend::RenderPass,
}

impl std::fmt::Debug for EguiRenderPassResource {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("EguiRenderPassResource")
			.field("render_pass", &"..")
			.finish()
	}
}

/// Paint jobs resource
#[derive(Debug)]
pub struct EguiPaintJobsResource {
	paint_jobs: FetchUpdate<Vec<egui::ClippedMesh>>,
}

/// Repaint signal
// Note: We paint egui every frame, so this isn't required currently, but
//       we should take it into consideration eventually.
#[derive(Clone, Copy, Debug)]
pub struct RepaintSignal;

impl epi::backend::RepaintSignal for RepaintSignal {
	fn request_repaint(&self) {}
}
