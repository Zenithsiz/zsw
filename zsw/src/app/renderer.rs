//! Renderer

// Imports
use {
	super::settings_window::SettingsWindow,
	crate::{
		util::{
			extse::{CrossBeamChannelSelectSE, CrossBeamChannelSenderSE},
			MightBlock,
		},
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
	/// Will block in it's own event loop until [`Self::stop`] is called.
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
			match self.render(window, wgpu, panels, egui, settings_window) {
				Ok(should_exit) => match should_exit {
					true => break,
					false => (),
				},
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
	/// Returns `Ok(true)` if the close channel was received from.
	fn render(
		&self,
		window: &Window,
		wgpu: &Wgpu,
		panels: &Panels,
		egui: &Egui,
		settings_window: &SettingsWindow,
	) -> Result<bool, anyhow::Error> {
		// Get the egui render results
		// Note: The settings window shouldn't quit while we're alive.
		let paint_jobs = {
			let mut select = crossbeam::channel::Select::new();
			let img_idx = settings_window.select_paint_jobs(&mut select);
			let close_idx = select.recv(&self.close_rx);

			// DEADLOCK: Caller can call `Self::stop` for us to stop at any moment.
			let selected = select.select_se().allow::<MightBlock>();
			match selected.index() {
				idx if idx == img_idx => settings_window.paint_jobs_selected(selected),

				// Note: This can't return an `Err` because `self` owns a receiver
				idx if idx == close_idx => {
					selected.recv(&self.close_rx).expect("On-close sender was closed");
					return Ok(true);
				},
				_ => unreachable!(),
			}
		};

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
		.allow::<MightBlock>()?;

		Ok(false)
	}
}
