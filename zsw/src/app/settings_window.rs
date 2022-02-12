//! Settings window

// Lints
// `egui` returns a response on every operation, but we don't use them
#![allow(unused_results)]

// Imports
use {
	crate::{
		util::{
			extse::{CrossBeamChannelReceiverSE, CrossBeamChannelSenderSE},
			MightBlock,
		},
		Egui,
		PanelImageState,
		PanelState,
		Panels,
		Playlist,
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
	zsw_side_effect_macros::side_effect,
};

/// Inner data
struct Inner {
	/// If open
	open: bool,

	/// New panel state
	new_panel_state: NewPanelState,
}

impl Inner {
	/// Creates the inner data
	pub fn new(surface_size: PhysicalSize<u32>) -> Self {
		Self {
			open:            false,
			new_panel_state: NewPanelState::new(surface_size),
		}
	}
}

/// Settings window
pub struct SettingsWindow {
	/// Queued open click
	queued_open_click: AtomicCell<Option<PhysicalPosition<f64>>>,

	/// Paint jobs sender
	paint_jobs_tx: crossbeam::channel::Sender<Vec<egui::epaint::ClippedMesh>>,

	/// Paint jobs receiver
	paint_jobs_rx: crossbeam::channel::Receiver<Vec<egui::epaint::ClippedMesh>>,

	/// Closing sender
	close_tx: crossbeam::channel::Sender<()>,

	/// Closing receiver
	close_rx: crossbeam::channel::Receiver<()>,
}

impl SettingsWindow {
	/// Creates the settings window
	pub fn new() -> Self {
		// Note: Making the close channel unbounded is what allows us to not block
		//       in `Self::stop`.
		let (paint_jobs_tx, paint_jobs_rx) = crossbeam::channel::bounded(0);
		let (close_tx, close_rx) = crossbeam::channel::unbounded();

		Self {
			queued_open_click: AtomicCell::new(None),
			paint_jobs_tx,
			paint_jobs_rx,
			close_tx,
			close_rx,
		}
	}

	/// Runs the setting window
	///
	/// # Blocking
	/// Blocks until the receiver of `paint_jobs_tx` receives a value.
	#[allow(clippy::useless_transmute)] // `crossbeam::select` does it
	#[side_effect(MightBlock)]
	pub fn run(&self, wgpu: &Wgpu, egui: &Egui, window: &Window, panels: &Panels, playlist: &Playlist) -> () {
		// Create the inner data
		// TODO: Check if it's fine to call `wgpu.surface_size`
		let mut inner = Inner::new(wgpu.surface_size());

		loop {
			// Get the surface size
			let surface_size = wgpu.surface_size();

			// Draw egui
			let res = egui.draw(window, |ctx, frame| {
				self.draw(&mut inner, ctx, frame, surface_size, window, panels, playlist)
			});

			let paint_jobs = match res {
				Ok(paint_jobs) => paint_jobs,
				Err(err) => {
					log::warn!("Unable to draw egui: {err:?}");
					continue;
				},
			};

			// Then send the paint jobs
			// DEADLOCK: Caller can call `Self::stop` for us to stop at any moment.
			crossbeam::select! {
				// Try to send an image
				// Note: This can't return an `Err` because `self` owns a receiver
				send(self.paint_jobs_tx, paint_jobs) -> res => res.expect("Paint jobs receiver was closed"),

				// If we get anything in the close channel, break
				// Note: This can't return an `Err` because `self` owns a receiver
				recv(self.close_rx) -> res => {
					res.expect("On-close sender was closed");
					break
				},
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

	/// Retrieves the paint jobs for the next frame
	///
	/// # Blocking
	/// Blocks until [`Self::run`] is running.
	#[side_effect(MightBlock)]
	pub fn paint_jobs(&self) -> Vec<egui::epaint::ClippedMesh> {
		// Note: This can't return an `Err` because `self` owns a sender
		// DEADLOCK: Caller ensures `Self::run` will eventually run.
		self.paint_jobs_rx
			.recv_se()
			.allow::<MightBlock>()
			.expect("Paint jobs sender was closed")
	}

	/// Draws the settings window
	fn draw(
		&self,
		inner: &mut Inner,
		ctx: &egui::CtxRef,
		_frame: &epi::Frame,
		surface_size: PhysicalSize<u32>,
		window: &Window,
		panels: &Panels,
		playlist: &Playlist,
	) -> Result<(), anyhow::Error> {
		// Create the base settings window
		let mut settings_window = egui::Window::new("Settings");

		// If we have any queued click, summon the window there
		if let Some(cursor_pos) = self.queued_open_click.take() {
			// Adjust cursor pos to account for the scale factor
			let scale_factor = window.scale_factor();
			let cursor_pos = cursor_pos.to_logical(scale_factor);

			// Then set the current position and that we're open
			settings_window = settings_window.current_pos(egui::pos2(cursor_pos.x, cursor_pos.y));
			inner.open = true;
		}

		// Then render it
		settings_window.open(&mut inner.open).show(ctx, |ui| {
			// DEADLOCK: We ensure we don't block within the callback.
			let mut panel_idx = 0;
			panels
				.for_each_mut::<_, ()>(|panel| {
					ui.collapsing(format!("Panel {panel_idx}"), |ui| {
						ui.add(PanelWidget::new(panel, surface_size));
					});

					panel_idx += 1;
				})
				.allow::<MightBlock>();
			ui.collapsing("Add panel", |ui| {
				ui.horizontal(|ui| {
					ui.label("Geometry");
					self::draw_rect(ui, &mut inner.new_panel_state.geometry, surface_size);
				});

				ui.horizontal(|ui| {
					ui.label("Fade point");
					egui::Slider::new(&mut inner.new_panel_state.fade_point, 0.5..=1.0).ui(ui);
				});

				ui.horizontal(|ui| {
					ui.label("Duration");
					egui::Slider::new(&mut inner.new_panel_state.duration_secs, 0.5..=180.0).ui(ui);
				});

				if ui.button("Add").clicked() {
					panels.add_panel(PanelState::new(
						inner.new_panel_state.geometry,
						PanelImageState::Empty,
						Duration::from_secs_f32(inner.new_panel_state.duration_secs),
						inner.new_panel_state.fade_point,
					));
				}
			});

			ui.horizontal(|ui| {
				ui.label("Re-scan directory");
				if ui.button("📁").clicked() {
					let file_dialog = native_dialog::FileDialog::new().show_open_single_dir();
					match file_dialog {
						Ok(file_dialog) => {
							if let Some(path) = file_dialog {
								// Set the root path
								playlist.clear();
								playlist.add_dir(&path);

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

	/// Queues an open click
	pub fn queue_open_click(&self, cursor_pos: Option<PhysicalPosition<f64>>) {
		self.queued_open_click.store(cursor_pos);
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
	panel: &'panel mut PanelState,

	/// Surface size
	surface_size: PhysicalSize<u32>,
}

impl<'panel> PanelWidget<'panel> {
	/// Creates a panel widget
	pub fn new(panel: &'panel mut PanelState, surface_size: PhysicalSize<u32>) -> Self {
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
