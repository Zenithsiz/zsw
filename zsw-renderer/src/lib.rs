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
)]

// Imports
use {
	anyhow::Context,
	async_lock::Mutex,
	pollster::FutureExt,
	std::{thread, time::Duration},
	winit::window::Window,
	zsw_egui::Egui,
	zsw_img::ImageLoader,
	zsw_panels::Panels,
	zsw_settings_window::SettingsWindow,
	zsw_side_effect_macros::side_effect,
	zsw_util::{extse::AsyncLockMutexSe, MightBlock},
	zsw_wgpu::Wgpu,
};

/// Renderer
#[derive(Debug)]
pub struct Renderer {
	/// Frame timings
	frame_timings: Mutex<FrameTimings>,
}

impl Renderer {
	/// Creates a new renderer
	#[must_use]
	pub fn new() -> Self {
		Self {
			frame_timings: Mutex::new(FrameTimings::new()),
		}
	}

	/// Runs the renderer
	///
	/// # Locking
	/// [`zsw_panels::PanelsLock`]
	/// [`zsw_wgpu::SurfaceLock`]
	/// - [`zsw_panels::PanelsLock`]
	/// - [`zsw_egui::RenderPassLock`]
	///   - [`zsw_egui::PlatformLock`]
	#[side_effect(MightBlock)]
	pub async fn run<'window, 'wgpu, 'egui, 'panels>(
		&self,
		window: &Window,
		wgpu: &'wgpu Wgpu<'window>,
		panels: &Panels,
		egui: &'egui Egui,
		image_loader: &ImageLoader,
		settings_window: &SettingsWindow,
	) -> ! {
		// Duration we're sleeping
		let sleep_duration = Duration::from_secs_f32(1.0 / 60.0);

		loop {
			let ((update_duration, render_duration), total_duration) = zsw_util::measure!({
				// Update
				// DEADLOCK: Caller ensures we can lock it
				let (_, update_duration) = zsw_util::measure!(Self::update(wgpu, panels, image_loader)
					.await
					.allow::<MightBlock>()
					.map_err(|err| log::warn!("Unable to update: {err:?}")));

				// Render
				// DEADLOCK: Caller ensures we can lock it
				let (_, render_duration) =
					zsw_util::measure!(Self::render(window, wgpu, panels, egui, settings_window)
						.await
						.allow::<MightBlock>()
						.map_err(|err| log::warn!("Unable to render: {err:?}")));

				(update_duration, render_duration)
			});

			// Update our frame timings
			// DEADLOCK: We don't hold any locks during locking
			self.frame_timings
				.lock_se()
				.await
				.allow::<MightBlock>()
				.add(FrameTiming {
					update: update_duration,
					render: render_duration,
					total:  total_duration,
				});

			// Then sleep until next frame
			// TODO: Await while sleeping
			if let Some(duration) = sleep_duration.checked_sub(total_duration) {
				thread::sleep(duration);
			}
		}
	}

	/// Returns all frame timings
	pub async fn frame_timings(&self) -> [FrameTiming; 60] {
		// DEADLOCK: We don't hold any locks during locking
		self.frame_timings.lock_se().await.allow::<MightBlock>().timings
	}

	/// Updates all panels
	///
	/// # Locking
	/// [`zsw_panels::PanelsLock`]
	#[side_effect(MightBlock)]
	async fn update<'window, 'panels>(
		wgpu: &Wgpu<'window>,
		panels: &'panels Panels,
		image_loader: &ImageLoader,
	) -> Result<(), anyhow::Error> {
		// DEADLOCK: Caller ensures we can lock it
		let mut panels_lock = panels.lock_panels().await.allow::<MightBlock>();

		// Updates all panels
		panels.update_all(&mut panels_lock, wgpu, image_loader)
	}

	/// Renders
	///
	/// # Locking
	/// [`zsw_wgpu::SurfaceLock`]
	/// - [`zsw_panels::PanelsLock`]
	/// - [`zsw_egui::RenderPassLock`]
	///   - [`zsw_egui::PlatformLock`]
	#[side_effect(MightBlock)]
	async fn render<'window, 'wgpu, 'egui, 'panels>(
		window: &Window,
		wgpu: &'wgpu Wgpu<'window>,
		panels: &'panels Panels,
		egui: &'egui Egui,
		settings_window: &SettingsWindow,
	) -> Result<(), anyhow::Error> {
		// Get the egui render results
		// DEADLOCK: We don't hold the `wgpu::SurfaceLock` lock from `wgpu`.
		//           Caller ensures we can lock it.
		let paint_jobs = settings_window.paint_jobs(wgpu).await.allow::<MightBlock>();

		// Lock the wgpu surface
		// DEADLOCK: Caller ensures we can lock it
		let mut surface_lock = wgpu.lock_surface().await.allow::<MightBlock>();

		// Then render
		wgpu.render(&mut surface_lock, |encoder, surface_view, surface_size| {
			// Render the panels
			{
				// DEADLOCK: Caller ensures we can lock it after the surface
				// TODO: Not block on this
				let panels_lock = panels.lock_panels().block_on().allow::<MightBlock>();

				panels
					.render(&panels_lock, wgpu.queue(), encoder, surface_view, surface_size)
					.context("Unable to render panels")?;
			}

			#[allow(clippy::cast_possible_truncation)] // Unfortunately `egui` takes an `f32`
			let screen_descriptor = egui_wgpu_backend::ScreenDescriptor {
				physical_width:  surface_size.width,
				physical_height: surface_size.height,
				scale_factor:    window.scale_factor() as f32,
			};
			let device = wgpu.device();
			let queue = wgpu.queue();

			// DEADLOCK: Caller ensures we can lock it after the wgpu surface lock
			// TODO: Not block on this
			let mut render_pass_lock = egui.lock_render_pass().block_on().allow::<MightBlock>();
			egui.do_render_pass(&mut render_pass_lock, |egui_render_pass| {
				let font_image = {
					// DEADLOCK: Caller ensures we can lock it after the egui render pass lock
					// TODO: Not block on this
					let platform_lock = egui.lock_platform().block_on().allow::<MightBlock>();
					egui.font_image(&platform_lock)
				};

				egui_render_pass.update_texture(device, queue, &font_image);
				egui_render_pass.update_user_textures(device, queue);
				egui_render_pass.update_buffers(device, queue, &paint_jobs, &screen_descriptor);

				// Record all render passes.
				egui_render_pass
					.execute(encoder, surface_view, &paint_jobs, &screen_descriptor, None)
					.context("Unable to render egui")
			})
		})
	}
}

impl Default for Renderer {
	fn default() -> Self {
		Self::new()
	}
}

/// Frame timings
#[derive(Clone, Copy, Debug)]
pub struct FrameTimings {
	/// Timings
	timings: [FrameTiming; 60],

	/// Current index
	cur_idx: usize,
}

impl FrameTimings {
	// Creates
	fn new() -> Self {
		Self {
			timings: [FrameTiming::default(); 60],
			cur_idx: 0,
		}
	}

	/// Adds a new frame timing
	pub fn add(&mut self, timing: FrameTiming) {
		self.timings[self.cur_idx] = timing;
		self.cur_idx = (self.cur_idx + 1) % 60;
	}
}

/// Frame timing
#[derive(Clone, Copy, Default, Debug)]
pub struct FrameTiming {
	/// Update
	pub update: Duration,

	/// Render
	// TODO: Split into `FrameRenderTiming`
	pub render: Duration,

	/// Total
	pub total: Duration,
}
