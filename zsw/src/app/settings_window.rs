//! Settings window

// Lints
// `egui` returns a response on every operation, but we don't use them
#![allow(unused_results)]


// Imports
use {
	crate::{
		paths,
		util::{MightDeadlock, WithSideEffect},
		Egui,
		Panel,
		PanelState,
		Panels,
		Rect,
		Wgpu,
	},
	cgmath::{Point2, Vector2},
	crossbeam::atomic::AtomicCell,
	egui::Widget,
	std::time::Duration,
	winit::{
		dpi::{PhysicalPosition, PhysicalSize},
		window::Window,
	},
};

/// Settings window
pub struct SettingsWindow {
	/// If open
	open: bool,

	/// New panel state
	new_panel_state: NewPanelState,
}

impl SettingsWindow {
	/// Creates the settings window
	pub fn new(surface_size: PhysicalSize<u32>) -> Self {
		Self {
			open:            false,
			new_panel_state: NewPanelState::new(surface_size),
		}
	}

	/// Runs the setting window
	///
	/// # Deadlock
	/// Cannot be called from within `Wgpu::Render`
	// TODO: Not use a channel, but instead something else
	pub fn run(
		mut self,
		wgpu: &Wgpu,
		egui: &Egui,
		window: &Window,
		panels: &Panels,
		paths_distributer: &paths::Distributer,
		queued_settings_window_open_click: &AtomicCell<Option<PhysicalPosition<f64>>>,
		paint_jobs_tx: &crossbeam::channel::Sender<Vec<egui::epaint::ClippedMesh>>,
	) -> WithSideEffect<(), MightDeadlock> {
		loop {
			// Get the surface size
			// TODO: This can deadlock if put inside the `egui.draw` closure.
			// DEADLOCK: We aren't calling it from within `Wgpu::render`.
			let surface_size = wgpu.surface_size().allow::<MightDeadlock>();

			// Draw egui
			let res = egui.draw(window, |ctx, frame| {
				self.draw(
					ctx,
					frame,
					surface_size,
					window,
					panels,
					paths_distributer,
					queued_settings_window_open_click,
				)
			});

			let paint_jobs = match res {
				Ok(paint_jobs) => paint_jobs,
				Err(err) => {
					log::warn!("Unable to draw egui: {err:?}");
					continue;
				},
			};

			// Then send the paint jobs
			if paint_jobs_tx.send(paint_jobs).is_err() {
				log::info!("Renderer thread quit, quitting");
				break;
			}
		}

		WithSideEffect::new(())
	}

	/// Draws the settings window
	fn draw(
		&mut self,
		ctx: &egui::CtxRef,
		_frame: &epi::Frame,
		surface_size: PhysicalSize<u32>,
		window: &Window,
		panels: &Panels,
		paths_distributer: &paths::Distributer,
		queued_settings_window_open_click: &AtomicCell<Option<PhysicalPosition<f64>>>,
	) -> Result<(), anyhow::Error> {
		// Create the base settings window
		let mut settings_window = egui::Window::new("Settings");

		// If we have any queued click, summon the window there
		if let Some(cursor_pos) = queued_settings_window_open_click.take() {
			// Adjust cursor pos to account for the scale factor
			let scale_factor = window.scale_factor();
			let cursor_pos = cursor_pos.to_logical(scale_factor);

			// Then set the current position and that we're open
			settings_window = settings_window.current_pos(egui::pos2(cursor_pos.x, cursor_pos.y));
			self.open = true;
		}

		// Then render it
		settings_window.open(&mut self.open).show(ctx, |ui| {
			let mut panel_idx = 0;
			panels.for_each_mut::<_, ()>(|panel| {
				ui.collapsing(format!("Panel {panel_idx}"), |ui| {
					ui.add(PanelWidget::new(panel, surface_size));
				});

				panel_idx += 1;
			});
			ui.collapsing("Add panel", |ui| {
				ui.horizontal(|ui| {
					ui.label("Geometry");
					self::draw_rect(ui, &mut self.new_panel_state.geometry, surface_size);
				});

				ui.horizontal(|ui| {
					ui.label("Fade point");
					egui::Slider::new(&mut self.new_panel_state.fade_point, 0.5..=1.0).ui(ui);
				});

				ui.horizontal(|ui| {
					ui.label("Duration");
					egui::Slider::new(&mut self.new_panel_state.duration_secs, 0.5..=180.0).ui(ui);
				});

				if ui.button("Add").clicked() {
					panels.add_panel(Panel::new(
						self.new_panel_state.geometry,
						PanelState::Empty,
						Duration::from_secs_f32(self.new_panel_state.duration_secs),
						self.new_panel_state.fade_point,
					));
				}
			});

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

/// New panel state
struct NewPanelState {
	/// Geometry
	geometry: Rect<u32>,

	/// Duration seconds
	duration_secs: f32,

	/// Fade point
	fade_point: f32,
}

impl NewPanelState {
	fn new(surface_size: PhysicalSize<u32>) -> Self {
		Self {
			geometry:      Rect {
				pos:  Point2::new(0, 0),
				size: Vector2::new(surface_size.width, surface_size.height),
			},
			duration_secs: 15.0,
			fade_point:    0.95,
		}
	}
}

/// Panel widget
#[derive(Debug)]
pub struct PanelWidget<'panel> {
	/// The panel
	panel: &'panel mut Panel,

	/// Surface size
	surface_size: PhysicalSize<u32>,
}

impl<'panel> PanelWidget<'panel> {
	/// Creates a panel widget
	pub fn new(panel: &'panel mut Panel, surface_size: PhysicalSize<u32>) -> Self {
		Self { panel, surface_size }
	}
}

impl<'panel> egui::Widget for PanelWidget<'panel> {
	fn ui(self, ui: &mut egui::Ui) -> egui::Response {
		ui.horizontal(|ui| {
			ui.label("Geometry");
			self::draw_rect(ui, &mut self.panel.geometry, self.surface_size);
		});

		ui.horizontal(|ui| {
			ui.label("Progress");
			egui::Slider::new(&mut self.panel.progress, 0.0..=0.99).ui(ui);
		});

		ui.horizontal(|ui| {
			ui.label("Fade point");
			egui::Slider::new(&mut self.panel.fade_point, 0.5..=1.0).ui(ui);
		});

		ui.horizontal(|ui| {
			ui.label("Duration");

			let mut seconds = self.panel.image_duration.as_secs_f32();
			egui::Slider::new(&mut seconds, 0.5..=180.0).ui(ui);
			self.panel.image_duration = Duration::from_secs_f32(seconds);
		});

		// TODO: Return more than just the skip button here
		ui.horizontal(|ui| {
			ui.label("Skip");
			if ui.button("🔄").clicked() {
				self.panel.progress = 1.0;
			}
		})
		.response
	}
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
