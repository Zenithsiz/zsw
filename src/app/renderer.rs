//! Renderer

// Modules
mod settings_window;

// Imports
use {
	self::settings_window::SettingsWindow,
	crate::{paths, Egui, ImageLoader, Panels, PanelsRenderer, Wgpu},
	anyhow::Context,
	crossbeam::atomic::AtomicCell,
	std::{thread, time::Duration},
	winit::{dpi::PhysicalPosition, window::Window},
};

/// Renderer
pub struct Renderer<'a> {
	/// Window
	window: &'a Window,

	/// Wgpu
	wgpu: &'a Wgpu<'a>,

	/// Path distributer
	paths_distributer: &'a paths::Distributer,

	/// Image loader
	image_loader: &'a ImageLoader,

	/// Panels renderer
	panels_renderer: &'a PanelsRenderer,

	/// Panels
	panels: &'a Panels,

	/// Egui
	egui: &'a Egui,

	/// Settings window
	settings_window: SettingsWindow<'a>,
}

impl<'a> Renderer<'a> {
	/// Creates a new renderer
	pub fn new(
		window: &'a Window,
		wgpu: &'a Wgpu,
		paths_distributer: &'a paths::Distributer,
		image_loader: &'a ImageLoader,
		panels_renderer: &'a PanelsRenderer,
		panels: &'a Panels,
		egui: &'a Egui,
		queued_settings_window_open_click: &'a AtomicCell<Option<PhysicalPosition<f64>>>,
	) -> Self {
		Self {
			window,
			wgpu,
			paths_distributer,
			image_loader,
			panels_renderer,
			panels,
			egui,
			settings_window: SettingsWindow::new(wgpu.surface_size(), queued_settings_window_open_click),
		}
	}

	/// Runs the renderer
	pub fn run(&mut self) {
		// Duration we're sleep
		let sleep_duration = Duration::from_secs_f32(1.0 / 60.0);

		loop {
			// Update
			// Note: The update is only useful for displaying, so there's no use
			//       in running it in another thread.
			//       Especially given that `update` doesn't block.
			let (res, frame_duration) = crate::util::measure(|| self.update());
			match res {
				Ok(()) => log::trace!(target: "zsw::perf", "Took {frame_duration:?} to update"),
				Err(err) => log::warn!("Unable to update: {err:?}"),
			};

			// Render
			let (res, frame_duration) = crate::util::measure(|| self.render());
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
	fn update(&mut self) -> Result<(), anyhow::Error> {
		self.panels.for_each_mut(|panel| {
			if let Err(err) = panel.update(self.wgpu, self.panels_renderer, self.image_loader) {
				log::warn!("Unable to update panel: {err:?}");
			}

			Ok(())
		})
	}

	/// Renders
	fn render(&mut self) -> Result<(), anyhow::Error> {
		// Draw egui
		// TODO: When this is moved to it's own thread, regardless of issues with
		//       synchronizing the platform, we should synchronize the drawing to ensure
		//       we don't draw twice without displaying, as the first draw would never be
		//       visible to the user.
		let paint_jobs = self
			.egui
			.draw(self.window, |ctx, frame| {
				self.settings_window.draw(
					ctx,
					frame,
					self.wgpu.surface_size(),
					self.window,
					self.panels,
					self.paths_distributer,
				)
			})
			.context("Unable to draw egui")?;

		self.wgpu.render(|encoder, surface_view, surface_size| {
			// Render the panels
			self.panels_renderer
				.render(self.panels, self.wgpu.queue(), encoder, surface_view, surface_size)
				.context("Unable to render panels")?;

			// Render egui
			#[allow(clippy::cast_possible_truncation)] // Unfortunately `egui` takes an `f32`
			let screen_descriptor = egui_wgpu_backend::ScreenDescriptor {
				physical_width:  surface_size.width,
				physical_height: surface_size.height,
				scale_factor:    self.window.scale_factor() as f32,
			};
			let device = self.wgpu.device();
			let queue = self.wgpu.queue();
			let mut egui_render_pass = self.egui.render_pass().lock();

			// TODO: Check if it's fine to get the platform here without synchronizing
			//       with the drawing.
			let egui_platform = self.egui.platform().lock();
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
