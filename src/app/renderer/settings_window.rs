//! Settings window

// Lints
#![allow(unused_results)] // `egui` returns a response on every operation, but we don't use them

// Imports
use {
	crate::{paths, Panel, PanelState, Rect},
	cgmath::{Point2, Vector2},
	crossbeam::atomic::AtomicCell,
	egui::Widget,
	parking_lot::Mutex,
	std::{mem, time::Duration},
	winit::{
		dpi::{PhysicalPosition, PhysicalSize},
		window::Window,
	},
};

/// Settings window
pub struct SettingsWindow<'a> {
	/// Queued settings window open click
	queued_settings_window_open_click: &'a AtomicCell<Option<PhysicalPosition<f64>>>,

	/// If open
	open: bool,

	/// New panel parameters
	new_panel_parameters: (Rect<u32>, f32, f32),
}

impl<'a> SettingsWindow<'a> {
	/// Creates the settings window
	pub fn new(queued_settings_window_open_click: &'a AtomicCell<Option<PhysicalPosition<f64>>>) -> Self {
		Self {
			queued_settings_window_open_click,
			open: false,
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

	/// Draws the settings window
	pub fn draw(
		&mut self,
		ctx: &egui::CtxRef,
		_frame: &epi::Frame,
		surface_size: PhysicalSize<u32>,
		window: &Window,
		panels: &Mutex<Vec<Panel>>,
		paths_distributer: &paths::Distributer,
	) -> Result<(), anyhow::Error> {
		// Create the base settings window
		let mut settings_window = egui::Window::new("Settings");

		// If we have any queued click, summon the window there
		if let Some(cursor_pos) = self.queued_settings_window_open_click.take() {
			// Adjust cursor pos to account for the scale factor
			let scale_factor = window.scale_factor();
			let cursor_pos = cursor_pos.to_logical(scale_factor);

			// Then set the current position and that we're open
			settings_window = settings_window.current_pos(egui::pos2(cursor_pos.x, cursor_pos.y));
			self.open = true;
		}

		// Then render it
		settings_window.open(&mut self.open).show(ctx, |ui| {
			let mut panels = panels.lock();
			for (idx, panel) in panels.iter_mut().enumerate() {
				ui.collapsing(format!("Panel {idx}"), |ui| {
					self::draw_panel(ui, panel, surface_size);
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
				let cur_root_path = paths_distributer.root_path();

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
								paths_distributer.set_root_path(root_path);

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

/// Draws a panel
fn draw_panel(ui: &mut egui::Ui, panel: &mut Panel, surface_size: PhysicalSize<u32>) {
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

	ui.horizontal(|ui| {
		ui.label("Skip");
		if ui.button("🔄").clicked() {
			//panel.state = PanelState::Empty;
			panel.progress = 1.0;
		}
	});
}


/// Draws a geometry rectangle
fn draw_rect(ui: &mut egui::Ui, geometry: &mut Rect<u32>, max_size: PhysicalSize<u32>) {
	// Calculate the limits
	// TODO: If two values are changed at the same time, during 1 frame it's
	//       possible for the values to be out of range.
	let max_width = max_size.width;
	let max_height = max_size.height;
	let max_x = max_size.width.saturating_sub(geometry.size.x);
	let max_y = max_size.height.saturating_sub(geometry.size.y);

	// new_panel_parameters

	egui::DragValue::new(&mut geometry.size.x)
		.clamp_range(0..=max_width)
		.speed(10)
		.ui(ui);
	ui.label("x");
	egui::DragValue::new(&mut geometry.size.y)
		.clamp_range(0..=max_height)
		.speed(10)
		.ui(ui);
	ui.label("+");
	egui::DragValue::new(&mut geometry.pos.x)
		.clamp_range(0..=max_x)
		.speed(10)
		.ui(ui);
	ui.label("+");
	egui::DragValue::new(&mut geometry.pos.y)
		.clamp_range(0..=max_y)
		.speed(10)
		.ui(ui);
}
