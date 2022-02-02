//! Renderer

// Modules
mod settings_window;

// Imports
use {
	self::settings_window::SettingsWindow,
	crate::{img::ImageReceiver, paths, Egui, Panels, PanelsRenderer, Wgpu},
	anyhow::Context,
	crossbeam::atomic::AtomicCell,
	std::{
		sync::atomic::{self, AtomicBool},
		thread,
		time::Duration,
	},
	winit::{dpi::PhysicalPosition, window::Window},
};

/// Renderer
pub struct Renderer {
	/// Image receiver
	image_receiver: ImageReceiver,

	/// Settings window
	settings_window: SettingsWindow,
}

impl Renderer {
	/// Creates a new renderer
	pub fn new(wgpu: &Wgpu, image_receiver: ImageReceiver) -> Self {
		Self {
			image_receiver,
			settings_window: SettingsWindow::new(wgpu.surface_size()),
		}
	}

	/// Runs the renderer
	pub fn run(
		mut self,
		window: &Window,
		wgpu: &Wgpu,
		paths_distributer: &paths::Distributer,
		panels_renderer: &PanelsRenderer,
		panels: &Panels,
		egui: &Egui,
		queued_settings_window_open_click: &AtomicCell<Option<PhysicalPosition<f64>>>,
		should_stop: &AtomicBool,
	) {
		// Duration we're sleeping
		let sleep_duration = Duration::from_secs_f32(1.0 / 60.0);

		while !should_stop.load(atomic::Ordering::Relaxed) {
			// Update
			// Note: The update is only useful for displaying, so there's no use
			//       in running it in another thread.
			//       Especially given that `update` doesn't block.
			let (res, frame_duration) = crate::util::measure(|| self.update(wgpu, panels_renderer, panels));
			match res {
				Ok(()) => log::trace!(target: "zsw::perf", "Took {frame_duration:?} to update"),
				Err(err) => log::warn!("Unable to update: {err:?}"),
			};

			// Render
			let (res, frame_duration) = crate::util::measure(|| {
				self.render(
					window,
					wgpu,
					paths_distributer,
					panels_renderer,
					panels,
					egui,
					queued_settings_window_open_click,
				)
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

	/// Updates all panels
	fn update(&mut self, wgpu: &Wgpu, panels_renderer: &PanelsRenderer, panels: &Panels) -> Result<(), anyhow::Error> {
		panels.for_each_mut(|panel| {
			if let Err(err) = panel.update(wgpu, panels_renderer, &self.image_receiver) {
				log::warn!("Unable to update panel: {err:?}");
			}

			Ok(())
		})
	}

	/// Renders
	fn render(
		&mut self,
		window: &Window,
		wgpu: &Wgpu,
		paths_distributer: &paths::Distributer,
		panels_renderer: &PanelsRenderer,
		panels: &Panels,
		egui: &Egui,
		queued_settings_window_open_click: &AtomicCell<Option<PhysicalPosition<f64>>>,
	) -> Result<(), anyhow::Error> {
		// Draw egui
		// TODO: When this is moved to it's own thread, regardless of issues with
		//       synchronizing the platform, we should synchronize the drawing to ensure
		//       we don't draw twice without displaying, as the first draw would never be
		//       visible to the user.
		let paint_jobs = egui
			.draw(window, |ctx, frame| {
				self.settings_window.draw(
					ctx,
					frame,
					wgpu.surface_size(),
					window,
					panels,
					paths_distributer,
					queued_settings_window_open_click,
				)
			})
			.context("Unable to draw egui")?;

		wgpu.render(|encoder, surface_view, surface_size| {
			// Render the panels
			panels_renderer
				.render(panels, wgpu.queue(), encoder, surface_view, surface_size)
				.context("Unable to render panels")?;

			// Render egui
			#[allow(clippy::cast_possible_truncation)] // Unfortunately `egui` takes an `f32`
			let screen_descriptor = egui_wgpu_backend::ScreenDescriptor {
				physical_width:  surface_size.width,
				physical_height: surface_size.height,
				scale_factor:    window.scale_factor() as f32,
			};
			let device = wgpu.device();
			let queue = wgpu.queue();
			let mut egui_render_pass = egui.render_pass().lock();

			// TODO: Check if it's fine to get the platform here without synchronizing
			//       with the drawing.
			let egui_platform = egui.platform().lock();
			egui_render_pass.update_texture(device, queue, &egui_platform.context().font_image());
			egui_render_pass.update_user_textures(device, queue);
			egui_render_pass.update_buffers(device, queue, &paint_jobs, &screen_descriptor);

			// Record all render passes.
			egui_render_pass
				.execute(encoder, surface_view, &paint_jobs, &screen_descriptor, None)
				.context("Unable to render egui")
		})
	}
}
