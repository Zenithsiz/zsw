//! Renderer

// Imports
use {
	super::settings_window::SettingsWindow,
	anyhow::Context,
	std::{
		thread,
		time::{Duration, Instant},
	},
	winit::window::Window,
	zsw_egui::Egui,
	zsw_img::ImageLoader,
	zsw_panels::Panels,
	zsw_util::MightBlock,
	zsw_wgpu::Wgpu,
};

/// Renderer
pub struct Renderer {}

impl Renderer {
	/// Creates a new renderer
	pub fn new() -> Self {
		Self {}
	}

	/// Runs the renderer
	pub async fn run(
		&self,
		window: &Window,
		wgpu: &Wgpu<'_>,
		panels: &Panels,
		egui: &Egui,
		image_loader: &ImageLoader,
		settings_window: &SettingsWindow,
	) -> ! {
		// Duration we're sleeping
		let sleep_duration = Duration::from_secs_f32(1.0 / 60.0);

		loop {
			let start_instant = Instant::now();

			// Update
			match Self::update(wgpu, panels, image_loader) {
				Ok(()) => (),
				Err(err) => log::warn!("Unable to update: {err:?}"),
			};

			// Render
			match Self::render(window, wgpu, panels, egui, settings_window).await {
				Ok(()) => (),
				Err(err) => log::warn!("Unable to render: {err:?}"),
			};

			// Then sleep until next frame
			// TODO: Await while sleeping
			let frame_duration = start_instant.elapsed();
			if let Some(duration) = sleep_duration.checked_sub(frame_duration) {
				thread::sleep(duration);
			}
		}
	}

	/// Updates all panels
	fn update(wgpu: &Wgpu, panels: &Panels, image_loader: &ImageLoader) -> Result<(), anyhow::Error> {
		// Updates all panels
		panels.update_all(wgpu, image_loader)
	}

	/// Renders
	async fn render(
		window: &Window,
		wgpu: &Wgpu<'_>,
		panels: &Panels,
		egui: &Egui,
		settings_window: &SettingsWindow,
	) -> Result<(), anyhow::Error> {
		// Get the egui render results
		let paint_jobs = settings_window.paint_jobs().await;

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

		Ok(())
	}
}
