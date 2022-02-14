//! Settings window

// Lints
// `egui` returns a response on every operation, but we don't use them
#![allow(unused_results)]

// Imports
use {
	cgmath::{Point2, Vector2},
	crossbeam::atomic::AtomicCell,
	egui::Widget,
	pollster::FutureExt,
	winit::{
		dpi::{PhysicalPosition, PhysicalSize},
		window::Window,
	},
	zsw_egui::Egui,
	zsw_panels::{Panel, PanelState, Panels},
	zsw_playlist::{Playlist, PlaylistImage},
	zsw_profiles::{Profile, Profiles},
	zsw_side_effect_macros::side_effect,
	zsw_util::{MightBlock, MightLock, Rect},
	zsw_wgpu::Wgpu,
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
	///
	/// # Locking
	/// Locks the `zsw_wgpu::SurfaceLock` lock on `wgpu`
	#[side_effect(MightLock<zsw_wgpu::SurfaceLock<'_, '_>>)]
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
		// DEADLOCK: Caller ensures we can lock it
		// TODO: Check if it's fine to call `wgpu.surface_size`
		let mut inner = {
			let surface_lock = wgpu.lock_surface().allow::<MightLock<zsw_wgpu::SurfaceLock>>();
			Inner::new(wgpu.surface_size(&surface_lock))
		};

		loop {
			// Get the surface size
			// DEADLOCK: Caller ensures we can lock it
			let surface_size = {
				let surface_lock = wgpu.lock_surface().allow::<MightLock<zsw_wgpu::SurfaceLock>>();
				wgpu.surface_size(&surface_lock)
			};

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
	///
	/// # Locking
	/// Locks the `zsw_wgpu::SurfaceLock` lock on `wgpu`
	// Note: Doesn't literally lock it, but the other side of the channel
	//       needs to lock it in order to progress, so it's equivalent
	#[side_effect(MightLock<zsw_wgpu::SurfaceLock<'_, '_>>)]
	pub async fn paint_jobs(&self) -> Vec<egui::epaint::ClippedMesh> {
		// Note: This can't return an `Err` because `self` owns a sender
		self.paint_jobs_rx.recv().await.expect("Paint jobs sender was closed")
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
			self::draw_settings_window(ui, &mut inner.new_panel_state, surface_size, panels, playlist, profiles);
		});

		Ok(())
	}

	/// Queues an open click
	pub fn queue_open_click(&self, cursor_pos: Option<PhysicalPosition<f64>>) {
		self.queued_open_click.store(cursor_pos);
	}
}

/// Draws the settings window
fn draw_settings_window(
	ui: &mut egui::Ui,
	new_panel_state: &mut NewPanelState,
	surface_size: PhysicalSize<u32>,
	panels: &Panels,
	playlist: &Playlist,
	profiles: &Profiles,
) {
	// Draw the panels header
	ui.collapsing("Panels", |ui| {
		self::draw_panels(ui, new_panel_state, surface_size, panels);
	});
	ui.collapsing("Playlist", |ui| {
		self::draw_playlist(ui, playlist);
	});
	ui.collapsing("Profile", |ui| {
		self::draw_profile(ui, panels, playlist, profiles);
	});
}

/// Draws the profile settings
fn draw_profile(ui: &mut egui::Ui, panels: &Panels, playlist: &Playlist, profiles: &Profiles) {
	// Get the profile to apply, if any
	// DEADLOCK: We don't block within it.
	let mut profile_to_apply = None;
	profiles
		.for_each::<_, ()>(|path, profile| {
			ui.horizontal(|ui| {
				ui.label(path.display().to_string());
				if ui.button("Apply").clicked() {
					profile_to_apply = Some(profile.clone());
				}
			});
		})
		.allow::<MightBlock>();

	// If we had any, apply it
	if let Some(profile) = profile_to_apply {
		profile.apply(playlist, panels).block_on();
	}

	// Draw the load button
	ui.horizontal(|ui| {
		ui.label("Load");
		if ui.button("📁").clicked() {
			let file_dialog = native_dialog::FileDialog::new().show_open_single_file();
			match file_dialog {
				Ok(file_dialog) =>
					if let Some(path) = file_dialog {
						match profiles.load(path.clone()) {
							Ok(profile) => log::info!("Successfully loaded profile: {profile:?}"),
							Err(err) => log::warn!("Unable to load profile at {path:?}: {err:?}"),
						}
					},
				Err(err) => log::warn!("Unable to ask user for new root directory: {err:?}"),
			}
		}
	});

	// Draw the save-as button
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
}

/// Draws the playlist settings
fn draw_playlist(ui: &mut egui::Ui, playlist: &Playlist) {
	// Draw the root path
	ui.horizontal(|ui| {
		// Show the current root path
		ui.label("Root path");
		ui.add_space(10.0);
		match playlist.root_path().block_on() {
			Some(root_path) => ui.label(root_path.display().to_string()),
			None => ui.label("<None>"),
		};

		// Then the change button
		if ui.button("📁").clicked() {
			// Ask for a file
			let file_dialog = native_dialog::FileDialog::new().show_open_single_dir();
			match file_dialog {
				Ok(file_dialog) => {
					if let Some(path) = file_dialog {
						// Set the root path
						// TODO: Not block on this?
						playlist.set_root_path(path).block_on();

						// TODO: Maybe reset both panels and loaders?
					}
				},
				Err(err) => log::warn!("Unable to ask user for new root directory: {err:?}"),
			}
		}
	});

	// Draw all paths in the pipeline
	ui.collapsing("Upcoming", |ui| {
		egui::ScrollArea::new([true, true]).max_height(500.0).show(ui, |ui| {
			playlist
				.peek_next(|image| match image {
					PlaylistImage::File(path) => {
						ui.label(path.display().to_string());
					},
				})
				.block_on();
		});
	});
}

/// Draws the panels settings
fn draw_panels(
	ui: &mut egui::Ui,
	new_panel_state: &mut NewPanelState,
	surface_size: PhysicalSize<u32>,
	panels: &Panels,
) {
	// Draw all panels in their own header
	// BLOCKING: TODO
	let mut panel_idx = 0;
	panels
		.for_each_mut::<_, ()>(|panel| {
			ui.collapsing(format!("Panel {panel_idx}"), |ui| {
				ui.add(PanelWidget::new(panel, surface_size));
			});

			panel_idx += 1;
		})
		.allow::<MightBlock>();

	// Draw the panel adder
	ui.collapsing("Add", |ui| {
		ui.horizontal(|ui| {
			ui.label("Geometry");
			self::draw_rect(ui, &mut new_panel_state.geometry, surface_size);
		});

		ui.horizontal(|ui| {
			ui.label("Fade progress");
			let min = new_panel_state.duration / 2;
			let max = new_panel_state.duration.saturating_sub(1);
			egui::Slider::new(&mut new_panel_state.fade_point, min..=max).ui(ui);
		});

		ui.horizontal(|ui| {
			ui.label("Max progress");
			egui::Slider::new(&mut new_panel_state.duration, 0..=10800).ui(ui);
		});

		if ui.button("Add").clicked() {
			panels.add_panel(Panel::new(
				new_panel_state.geometry,
				new_panel_state.duration,
				new_panel_state.fade_point,
			));
		}
	});
}

/// New panel state
struct NewPanelState {
	/// Geometry
	geometry: Rect<i32, u32>,

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
fn draw_rect(ui: &mut egui::Ui, geometry: &mut Rect<i32, u32>, max_size: PhysicalSize<u32>) {
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
