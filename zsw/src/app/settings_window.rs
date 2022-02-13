//! Settings window

// Lints
// `egui` returns a response on every operation, but we don't use them
#![allow(unused_results)]

// Imports
use {
	crate::{util::MightBlock, Egui, Panel, PanelState, Panels, Playlist, Profile, Profiles, Rect, Wgpu},
	cgmath::{Point2, Vector2},
	crossbeam::atomic::AtomicCell,
	egui::Widget,
	pollster::FutureExt,
	winit::{
		dpi::{PhysicalPosition, PhysicalSize},
		window::Window,
	},
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
	paint_jobs_tx: async_channel::Sender<Vec<egui::epaint::ClippedMesh>>,

	/// Paint jobs receiver
	paint_jobs_rx: async_channel::Receiver<Vec<egui::epaint::ClippedMesh>>,
}

impl SettingsWindow {
	/// Creates the settings window
	pub fn new() -> Self {
		// Note: Making the close channel unbounded is what allows us to not block
		//       in `Self::stop`.
		let (paint_jobs_tx, paint_jobs_rx) = async_channel::bounded(1);

		Self {
			queued_open_click: AtomicCell::new(None),
			paint_jobs_tx,
			paint_jobs_rx,
		}
	}

	/// Runs the setting window
	pub async fn run(
		&self,
		wgpu: &Wgpu<'_>,
		egui: &Egui,
		window: &Window,
		panels: &Panels,
		playlist: &Playlist,
		profiles: &Profiles,
	) -> ! {
		// Create the inner data
		// TODO: Check if it's fine to call `wgpu.surface_size`
		let mut inner = Inner::new(wgpu.surface_size());

		loop {
			// Get the surface size
			let surface_size = wgpu.surface_size();

			// Draw egui
			let res = egui.draw(window, |ctx, frame| {
				self.draw(&mut inner, ctx, frame, surface_size, window, panels, playlist, profiles)
			});

			let paint_jobs = match res {
				Ok(paint_jobs) => paint_jobs,
				Err(err) => {
					log::warn!("Unable to draw egui: {err:?}");
					continue;
				},
			};

			// Then send the paint jobs
			// DEADLOCK: TODO
			self.paint_jobs_tx
				.send(paint_jobs)
				.await
				.expect("Paint jobs receiver was closed");
		}
	}

	/// Retrieves the paint jobs for the next frame
	pub async fn paint_jobs(&self) -> Vec<egui::epaint::ClippedMesh> {
		// Note: This can't return an `Err` because `self` owns a sender
		self.paint_jobs_rx.recv().await.expect("Paint jobs sender was closed")
	}

	/// Draws the settings window
	#[allow(clippy::too_many_lines)] // TODO: Refactor
	fn draw(
		&self,
		inner: &mut Inner,
		ctx: &egui::CtxRef,
		_frame: &epi::Frame,
		surface_size: PhysicalSize<u32>,
		window: &Window,
		panels: &Panels,
		playlist: &Playlist,
		profiles: &Profiles,
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
			ui.collapsing("Panels", |ui| {
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

				// TODO: Remove, not very useful in it's current state
				ui.collapsing("Add", |ui| {
					ui.horizontal(|ui| {
						ui.label("Geometry");
						self::draw_rect(ui, &mut inner.new_panel_state.geometry, surface_size);
					});

					ui.horizontal(|ui| {
						ui.label("Fade progress");
						let min = inner.new_panel_state.duration / 2;
						let max = inner.new_panel_state.duration.saturating_sub(1);
						egui::Slider::new(&mut inner.new_panel_state.fade_point, min..=max).ui(ui);
					});

					ui.horizontal(|ui| {
						ui.label("Max progress");
						egui::Slider::new(&mut inner.new_panel_state.duration, 0..=10800).ui(ui);
					});

					if ui.button("Add").clicked() {
						panels.add_panel(Panel::new(
							inner.new_panel_state.geometry,
							inner.new_panel_state.duration,
							inner.new_panel_state.fade_point,
						));
					}
				});
			});

			ui.collapsing("Playlist", |ui| {
				ui.horizontal(|ui| {
					ui.label("Re-scan directory");
					if ui.button("📁").clicked() {
						let file_dialog = native_dialog::FileDialog::new().show_open_single_dir();
						match file_dialog {
							Ok(file_dialog) => {
								if let Some(path) = file_dialog {
									// Set the root path
									// TODO: Not block on this?
									async {
										playlist.clear().await;
										playlist.add_dir(path).await;
									}
									.block_on();

									// TODO: Reset all existing images and paths loaded from the
									//       old path distributer, maybe?
								}
							},
							Err(err) => log::warn!("Unable to ask user for new root directory: {err:?}"),
						}
					}
				});
			});

			ui.collapsing("Profile", |ui| {
				// DEADLOCK: TODO
				//           We currently block, but we could always just "find" the profile that was clicked
				//           and do it outside. Implement that.
				profiles
					.for_each::<_, ()>(|path, profile| {
						ui.horizontal(|ui| {
							ui.label(path.display().to_string());
							if ui.button("Apply").clicked() {
								// Set the root path and set the paths
								async {
									playlist.clear().await;
									playlist.add_dir(profile.root_path.clone()).await;
									panels.replace_panels(profile.panels.iter().copied());
								}
								.block_on();
							}
						});
					})
					.allow::<MightBlock>();

				ui.horizontal(|ui| {
					ui.label("Load");
					if ui.button("📁").clicked() {
						let file_dialog = native_dialog::FileDialog::new().show_open_single_file();
						match file_dialog {
							Ok(file_dialog) =>
								if let Some(path) = file_dialog {
									match profiles.load(path.clone()) {
										Ok(()) => (),
										Err(err) => log::warn!("Unable to load profile at {path:?}: {err:?}"),
									}
								},
							Err(err) => log::warn!("Unable to ask user for new root directory: {err:?}"),
						}
					}
				});

				ui.horizontal(|ui| {
					ui.label("Save As");
					if ui.button("📁").clicked() {
						let file_dialog = native_dialog::FileDialog::new().show_save_single_file();
						match file_dialog {
							Ok(file_dialog) =>
								if let Some(path) = file_dialog {
									let profile = Profile {
										root_path: match playlist.root_path().block_on() {
											Some(path) => path,
											None => {
												log::warn!("No root path was set");
												return;
											},
										},
										panels:    panels.panels(),
									};

									match profiles.save(path.clone(), profile) {
										Ok(()) => (),
										Err(err) => log::warn!("Unable to load profile at {path:?}: {err:?}"),
									}
								},
							Err(err) => log::warn!("Unable to ask user for new root directory: {err:?}"),
						}
					}
				});
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

	/// Max progress (in frames)
	duration: u64,

	/// Fade progress (in frames)
	fade_point: u64,
}

impl NewPanelState {
	fn new(surface_size: PhysicalSize<u32>) -> Self {
		#[allow(clippy::cast_sign_loss)] // It's positive
		Self {
			geometry:   Rect {
				pos:  Point2::new(0, 0),
				size: Vector2::new(surface_size.width, surface_size.height),
			},
			duration:   15 * 60,
			fade_point: (0.95 * 15.0 * 60.0) as u64,
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
			self::draw_rect(ui, &mut self.panel.panel.geometry, self.surface_size);
		});

		ui.horizontal(|ui| {
			ui.label("Cur progress");
			let max = self.panel.panel.duration.saturating_sub(1);
			egui::Slider::new(&mut self.panel.cur_progress, 0..=max).ui(ui);
		});

		ui.horizontal(|ui| {
			ui.label("Fade Point");
			let min = self.panel.panel.duration / 2;
			let max = self.panel.panel.duration.saturating_sub(1);
			egui::Slider::new(&mut self.panel.panel.fade_point, min..=max).ui(ui);
		});

		ui.horizontal(|ui| {
			ui.label("Duration");
			egui::Slider::new(&mut self.panel.panel.duration, 0..=10800).ui(ui);
		});

		// TODO: Return more than just the skip button here
		ui.horizontal(|ui| {
			ui.label("Skip");
			if ui.button("🔄").clicked() {
				self.panel.cur_progress = self.panel.panel.duration;
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
