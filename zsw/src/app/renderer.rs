//! Renderer

// Imports
use {
	super::settings_window::SettingsWindow,
	crate::{
		util::{extse::CrossBeamChannelSenderSE, MightBlock},
		Egui,
		ImageLoader,
		Panels,
		Wgpu,
	},
	anyhow::Context,
	std::{thread, time::Duration},
	winit::window::Window,
	zsw_side_effect_macros::side_effect,
};

/// Renderer
pub struct Renderer {
	/// Closing sender
	close_tx: crossbeam::channel::Sender<()>,

	/// Closing receiver
	close_rx: crossbeam::channel::Receiver<()>,
}

impl Renderer {
	/// Creates a new renderer
	pub fn new() -> Self {
		// Note: Making the close channel unbounded is what allows us to not block
		//       in `Self::stop`.
		let (close_tx, close_rx) = crossbeam::channel::unbounded();

		Self { close_tx, close_rx }
	}

	/// Runs the renderer
	///
	/// # Blocking
	/// Blocks waiting for a value from the sender of `paint_jobs_rx`.
	/// Deadlocks if called within a [`Wgpu::render`] callback.
	#[side_effect(MightBlock)]
	pub fn run(
		&self,
		window: &Window,
		wgpu: &Wgpu,
		panels: &Panels,
		egui: &Egui,
		image_loader: &ImageLoader,
		settings_window: &SettingsWindow,
	) -> () {
		// Duration we're sleeping
		let sleep_duration = Duration::from_secs_f32(1.0 / 60.0);

		// TODO: Use the stop channel better
		while self.close_rx.try_recv().is_err() {
			// Update
			// Note: The update is only useful for displaying, so there's no use
			//       in running it in another thread.
			//       Especially given that `update` doesn't block.
			let (res, frame_duration) = crate::util::measure(|| Self::update(wgpu, panels, image_loader));
			match res {
				Ok(()) => log::trace!(target: "zsw::perf", "Took {frame_duration:?} to update"),
				Err(err) => log::warn!("Unable to update: {err:?}"),
			};

			// Render
			// DEADLOCK: `Self::render` doesn't block
			let (res, frame_duration) = crate::util::measure(|| {
				Self::render(window, wgpu, panels, egui, settings_window).allow::<MightBlock>()
			});
			match res {
				Ok(()) => log::trace!(target: "zsw::perf", "Took {frame_duration:?} to render"),
				Err(err) => log::warn!("Unable to render: {err:?}"),
			};

			// Then sleep until next frame
			if let Some(duration) = sleep_duration.checked_sub(frame_duration) {
				thread::sleep(duration);
			}
		}
	}

	/// Stops the `run` loop
	pub fn stop(&self) {
		// Note: This can't return an `Err` because `self` owns a sender
		// DEADLOCK: The channel is unbounded, so this will not block.
		self.close_tx
			.send_se(())
			.allow::<MightBlock>()
			.expect("On-close receiver was closed");
	}

	/// Updates all panels
	fn update(wgpu: &Wgpu, panels: &Panels, image_loader: &ImageLoader) -> Result<(), anyhow::Error> {
		// Updates all panels
		panels.update_all(wgpu, image_loader)
	}

	/// Renders
	///
	/// # Blocking
	/// Blocks waiting for a value from the sender of `paint_jobs_rx`.
	/// Deadlocks if called within a [`Wgpu::render`] callback.
	#[side_effect(MightBlock)]
	fn render(
		window: &Window,
		wgpu: &Wgpu,
		panels: &Panels,
		egui: &Egui,
		settings_window: &SettingsWindow,
	) -> Result<(), anyhow::Error> {
		// Get the egui render results
		// Note: The settings window shouldn't quit while we're alive.
		// BLOCKING: TODO
		let paint_jobs = settings_window.paint_jobs().allow::<MightBlock>();

		// Then render
		// DEADLOCK: We ensure we don't block within [`Wgpu::render`].
		//           We also ensure we don't call it recursively.
		wgpu.render(|encoder, surface_view, surface_size| {
			// Render the panels
			panels
				.render(wgpu.queue(), encoder, surface_view, surface_size)
				.context("Unable to render panels")?;

			#[allow(clippy::cast_possible_truncation)] // Unfortunately `egui` takes an `f32`
			let screen_descriptor = egui_wgpu_backend::ScreenDescriptor {
				physical_width:  surface_size.width,
				physical_height: surface_size.height,
				scale_factor:    window.scale_factor() as f32,
			};
			let device = wgpu.device();
			let queue = wgpu.queue();

			// DEADLOCK: We ensure the callback doesn't block.
			egui.do_render_pass(|egui_render_pass| {
				// TODO: Check if it's fine to get the platform here without synchronizing
				//       with the drawing.
				egui_render_pass.update_texture(device, queue, &egui.font_image());
				egui_render_pass.update_user_textures(device, queue);
				egui_render_pass.update_buffers(device, queue, &paint_jobs, &screen_descriptor);

				// Record all render passes.
				egui_render_pass
					.execute(encoder, surface_view, &paint_jobs, &screen_descriptor, None)
					.context("Unable to render egui")
			})
			.allow::<MightBlock>()?;

			Ok(())
		})
		.allow::<MightBlock>()
	}
}
