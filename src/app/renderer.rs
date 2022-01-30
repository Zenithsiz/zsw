//! Renderer

// Imports
use {
	crate::{paths, Egui, ImageLoader, Panel, PanelState, PanelsRenderer, Rect, Wgpu},
	anyhow::Context,
	cgmath::{Point2, Vector2},
	crossbeam::atomic::AtomicCell,
	egui::Widget,
	parking_lot::Mutex,
	std::{mem, thread, time::Duration},
	winit::{
		dpi::{PhysicalPosition, PhysicalSize},
		window::Window,
	},
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
	panels: &'a Mutex<Vec<Panel>>,

	/// Egui
	egui: &'a Egui,

	/// Queued settings window open click
	queued_settings_window_open_click: &'a AtomicCell<Option<PhysicalPosition<f64>>>,

	/// If the settings window is currently open
	settings_window_open: bool,

	/// New panel parameters
	new_panel_parameters: (Rect<u32>, f32, f32),
}

impl<'a> Renderer<'a> {
	/// Creates a new renderer
	pub fn new(
		window: &'a Window,
		wgpu: &'a Wgpu,
		paths_distributer: &'a paths::Distributer,
		image_loader: &'a ImageLoader,
		panels_renderer: &'a PanelsRenderer,
		panels: &'a Mutex<Vec<Panel>>,
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
			queued_settings_window_open_click,
			settings_window_open: false,
			new_panel_parameters: (
				Rect {
					pos:  Point2::new(0, 0),
					size: Vector2::new(0, 0),
				},
				15.0,
				0.85,
			),
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
		let mut panels = self.panels.lock();
		for panel in &mut *panels {
			if let Err(err) = panel.update(self.wgpu, self.panels_renderer, self.image_loader) {
				log::warn!("Unable to update panel: {err:?}");
			}
		}

		Ok(())
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
				self.draw_egui(ctx, frame, self.wgpu.surface_size())
			})
			.context("Unable to draw egui")?;

		self.wgpu.render(|encoder, surface_view, surface_size| {
			// Render the panels
			let mut panels = self.panels.lock();
			self.panels_renderer
				.render(&mut *panels, self.wgpu.queue(), encoder, surface_view, surface_size)
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

	/// Draws egui app
	#[allow(unused_results)] // `egui` returns a response on every operation, but we don't use them
	fn draw_egui(
		&mut self,
		ctx: &egui::CtxRef,
		_frame: &epi::Frame,
		surface_size: PhysicalSize<u32>,
	) -> Result<(), anyhow::Error> {
		// Create the base settings window
		let mut settings_window = egui::Window::new("Settings");

		// If we have any queued click, summon the window there
		if let Some(cursor_pos) = self.queued_settings_window_open_click.take() {
			// Adjust cursor pos to account for the scale factor
			let scale_factor = self.window.scale_factor();
			let cursor_pos = cursor_pos.to_logical(scale_factor);

			// Then set the current position and that we're open
			settings_window = settings_window.current_pos(egui::pos2(cursor_pos.x, cursor_pos.y));
			self.settings_window_open = true;
		}

		// Then render it
		settings_window.open(&mut self.settings_window_open).show(ctx, |ui| {
			let mut panels = self.panels.lock();
			for (idx, panel) in panels.iter_mut().enumerate() {
				ui.collapsing(format!("Panel {idx}"), |ui| {
					// TODO: Make a macro to make this more readable
					ui.horizontal(|ui| {
						ui.label("Geometry");
						self::draw_rect(ui, &mut panel.geometry, surface_size);
					});
					ui.horizontal(|ui| {
						ui.label("Progress");
						egui::Slider::new(&mut panel.progress, 0.0..=0.99).ui(ui);
					});
					ui.horizontal(|ui| {
						ui.label("Fade point");
						egui::Slider::new(&mut panel.fade_point, 0.5..=1.0).ui(ui);
					});
					ui.horizontal(|ui| {
						ui.label("Duration");
						let mut seconds = panel.image_duration.as_secs_f32();
						egui::Slider::new(&mut seconds, 0.5..=180.0).ui(ui);
						panel.image_duration = Duration::from_secs_f32(seconds);
					});

					// On skip, skip the current panel
					// TODO: Do this properly
					ui.horizontal(|ui| {
						ui.label("Skip");
						if ui.button("🔄").clicked() {
							//panel.state = PanelState::Empty;
							panel.progress = 1.0;
						}
					});
				});
			}
			ui.collapsing("Add panel", |ui| {
				let (geometry, image_duration, fade_point) = &mut self.new_panel_parameters;

				ui.horizontal(|ui| {
					ui.label("Geometry");
					self::draw_rect(ui, geometry, surface_size);
				});

				ui.horizontal(|ui| {
					ui.label("Fade point");
					egui::Slider::new(fade_point, 0.5..=1.0).ui(ui);
				});

				ui.horizontal(|ui| {
					ui.label("Duration");
					egui::Slider::new(image_duration, 0.5..=180.0).ui(ui);
				});

				if ui.button("Add").clicked() {
					panels.push(Panel::new(
						*geometry,
						PanelState::Empty,
						Duration::from_secs_f32(*image_duration),
						*fade_point,
					));
				}
			});
			mem::drop(panels);

			ui.horizontal(|ui| {
				let cur_root_path = self.paths_distributer.root_path();

				ui.label("Root path");
				ui.label(cur_root_path.display().to_string());
				if ui.button("📁").clicked() {
					let file_dialog = native_dialog::FileDialog::new()
						.set_location(&*cur_root_path)
						.show_open_single_dir();
					match file_dialog {
						Ok(file_dialog) => {
							if let Some(root_path) = file_dialog {
								// Set the root path
								self.paths_distributer.set_root_path(root_path);

								// TODO: Reset all existing images and paths loaded from the
								//       old path distributer, maybe?
							}
						},
						Err(err) => log::warn!("Unable to ask user for new root directory: {err:?}"),
					}
				}
			});
		});

		Ok(())
	}
}

/// Draws a geometry rectangle
fn draw_rect(ui: &mut egui::Ui, geometry: &mut Rect<u32>, max_size: PhysicalSize<u32>) -> egui::Response {
	// Calculate the limits
	// TODO: If two values are changed at the same time, during 1 frame it's
	//       possible for the values to be out of range.
	let max_width = max_size.width;
	let max_height = max_size.height;
	let max_x = max_size.width.saturating_sub(geometry.size.x);
	let max_y = max_size.height.saturating_sub(geometry.size.y);

	// new_panel_parameters

	let mut response = egui::DragValue::new(&mut geometry.size.x)
		.clamp_range(0..=max_width)
		.speed(10)
		.ui(ui);
	response |= ui.label("x");
	response |= egui::DragValue::new(&mut geometry.size.y)
		.clamp_range(0..=max_height)
		.speed(10)
		.ui(ui);
	response |= ui.label("+");
	response |= egui::DragValue::new(&mut geometry.pos.x)
		.clamp_range(0..=max_x)
		.speed(10)
		.ui(ui);
	response |= ui.label("+");
	response |= egui::DragValue::new(&mut geometry.pos.y)
		.clamp_range(0..=max_y)
		.speed(10)
		.ui(ui);

	response
}
