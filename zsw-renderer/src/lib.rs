//! Renderer

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
	// This is too prevalent on generic functions, which we don't want to ALWAYS be `Send`
	clippy::future_not_send,
)]

// Imports
use {
	anyhow::Context,
	std::{mem, time::Duration},
	tokio::time::Instant,
	winit::window::Window,
	zsw_egui::Egui,
	zsw_img::ImageLoader,
	zsw_input::Input,
	zsw_panels::{Panels, PanelsResource},
	zsw_util::{ResourcesLock, ServicesContains},
	zsw_wgpu::{Wgpu, WgpuSurfaceResource},
};

/// Renderer
#[derive(Debug)]
#[allow(missing_copy_implementations)] // We're a service, we're not supposed to be copy
pub struct Renderer {}

impl Renderer {
	/// Creates a new renderer
	#[must_use]
	pub fn new() -> Self {
		Self {}
	}

	/// Runs the renderer
	///
	/// # Blocking
	/// Lock tree:
	/// [`zsw_panels::PanelsLock`] on `panels`
	/// [`zsw_wgpu::SurfaceLock`] on `wgpu`
	/// - [`zsw_panels::PanelsLock`] on `panels`
	/// - [`zsw_egui::RenderPassLock`] on `egui`
	///   - [`zsw_egui::PlatformLock`] on `egui`
	pub async fn run<S, R>(&self, services: &S, resources: &R) -> !
	where
		S: ServicesContains<Wgpu>
			+ ServicesContains<Egui>
			+ ServicesContains<Window>
			+ ServicesContains<Panels>
			+ ServicesContains<Input>
			+ ServicesContains<ImageLoader>,
		R: ResourcesLock<PanelsResource> + ResourcesLock<WgpuSurfaceResource>,
	{
		// Duration we're sleeping
		let sleep_duration = Duration::from_secs_f32(1.0 / 60.0);

		loop {
			let start_time = Instant::now();

			// Update
			// DEADLOCK: Caller ensures we can lock it
			if let Err(err) = Self::update(services, resources).await {
				log::warn!("Unable to update: {err:?}");
			}

			// Render
			// DEADLOCK: Caller ensures we can lock it
			if let Err(err) = Self::render(services, resources).await {
				log::warn!("Unable to render: {err:?}");
			};

			// Then sleep until next frame
			// TODO: Is it fine to measure time like this? asynchronously
			if let Some(duration) = sleep_duration.checked_sub(start_time.elapsed()) {
				tokio::time::sleep(duration).await;
			}
		}
	}

	/// Updates all panels
	///
	/// # Blocking
	/// Locks [`zsw_panels::PanelsLock`] on `panels`
	async fn update<S, R>(services: &S, resources: &R) -> Result<(), anyhow::Error>
	where
		S: ServicesContains<Wgpu> + ServicesContains<Panels> + ServicesContains<ImageLoader>,
		R: ResourcesLock<PanelsResource>,
	{
		// DEADLOCK: Caller ensures we can lock it
		let mut panels_resource = resources.resource::<PanelsResource>().await;

		// Updates all panels
		services.service::<Panels>().update_all(
			&mut panels_resource,
			services.service::<Wgpu>(),
			services.service::<ImageLoader>(),
		)
	}

	/// Renders
	///
	/// # Blocking
	/// Lock tree:
	/// [`zsw_wgpu::SurfaceLock`] on `wgpu`
	/// - [`zsw_panels::PanelsLock`] on `panels`
	/// - [`zsw_egui::PaintJobsLock`] on `egui`
	///   - [`zsw_egui::RenderPassLock`] on `egui`
	///     - [`zsw_egui::PlatformLock`] on `egui`
	async fn render<S, R>(services: &S, resources: &R) -> Result<(), anyhow::Error>
	where
		S: ServicesContains<Wgpu>
			+ ServicesContains<Egui>
			+ ServicesContains<Window>
			+ ServicesContains<Panels>
			+ ServicesContains<Input>,
		R: ResourcesLock<PanelsResource> + ResourcesLock<WgpuSurfaceResource>,
	{
		let wgpu = services.service::<Wgpu>();
		let egui = services.service::<Egui>();
		let window = services.service::<Window>();
		let panels = services.service::<Panels>();
		let input = services.service::<Input>();

		// Lock the wgpu surface
		// DEADLOCK: Caller ensures we can lock it
		let mut surface_resource = resources.resource::<WgpuSurfaceResource>().await;

		// Then render
		let surface_size = wgpu.surface_size(&surface_resource);
		let mut frame = wgpu
			.start_render(&mut surface_resource)
			.context("Unable to start render")?;

		// Render the panels
		{
			// DEADLOCK: Caller ensures we can lock it after the surface
			let panels_resource = resources.resource::<PanelsResource>().await;

			panels
				.render(
					input,
					&panels_resource,
					wgpu.queue(),
					&mut frame.encoder,
					&frame.surface_view,
					surface_size,
				)
				.context("Unable to render panels")?;
		}

		#[allow(clippy::cast_possible_truncation)] // Unfortunately `egui` takes an `f32`
		let screen_descriptor = egui_wgpu_backend::ScreenDescriptor {
			physical_width:  surface_size.width,
			physical_height: surface_size.height,
			scale_factor:    window.scale_factor() as f32,
		};

		// Get the egui render results
		// DEADLOCK: Caller ensures we can lock it.
		let paint_jobs_lock = egui.lock_paint_jobs().await;
		let paint_jobs = egui.paint_jobs(&paint_jobs_lock);

		// If we have any paint jobs, draw egui
		if !paint_jobs.is_empty() {
			// DEADLOCK: Caller ensures we can lock it after the wgpu surface lock
			let mut render_pass_lock = egui.lock_render_pass().await;
			let egui_render_pass = egui.render_pass(&mut render_pass_lock);

			let font_image = {
				// DEADLOCK: Caller ensures we can lock it after the egui render pass lock
				let platform_lock = egui.lock_platform().await;
				egui.font_image(&platform_lock)
			};

			egui_render_pass.update_texture(wgpu.device(), wgpu.queue(), &font_image);
			egui_render_pass.update_user_textures(wgpu.device(), wgpu.queue());
			egui_render_pass.update_buffers(wgpu.device(), wgpu.queue(), paint_jobs, &screen_descriptor);

			// Record all render passes.
			egui_render_pass
				.execute(
					&mut frame.encoder,
					&frame.surface_view,
					paint_jobs,
					&screen_descriptor,
					None,
				)
				.context("Unable to render egui")?;
		}

		mem::drop(paint_jobs_lock);
		wgpu.finish_render(frame);

		Ok(())
	}
}

impl Default for Renderer {
	fn default() -> Self {
		Self::new()
	}
}
