//! Settings window

// Features
#![feature(never_type, explicit_generic_args_with_impl_trait)]
// Lints
#![warn(
	clippy::pedantic,
	clippy::nursery,
	missing_copy_implementations,
	missing_debug_implementations,
	noop_method_call,
	unused_results
)]
#![deny(
	// We want to annotate unsafe inside unsafe fns
	unsafe_op_in_unsafe_fn,
	// We muse use `expect` instead
	clippy::unwrap_used
)]
#![allow(
	// Style
	clippy::implicit_return,
	clippy::multiple_inherent_impl,
	clippy::pattern_type_mismatch,
	// `match` reads easier than `if / else`
	clippy::match_bool,
	clippy::single_match_else,
	//clippy::single_match,
	clippy::self_named_module_files,
	clippy::items_after_statements,
	clippy::module_name_repetitions,
	// Performance
	clippy::suboptimal_flops, // We prefer readability
	// Some functions might return an error in the future
	clippy::unnecessary_wraps,
	// Due to working with windows and rendering, which use `u32` / `f32` liberally
	// and interchangeably, we can't do much aside from casting and accepting possible
	// losses, although most will be lossless, since we deal with window sizes and the
	// such, which will fit within a `f32` losslessly.
	clippy::cast_precision_loss,
	clippy::cast_possible_truncation,
	// We use proper error types when it matters what errors can be returned, else,
	// such as when using `anyhow`, we just assume the caller won't check *what* error
	// happened and instead just bubbles it up
	clippy::missing_errors_doc,
	// Too many false positives and not too important
	clippy::missing_const_for_fn,
	// This is a binary crate, so we don't expose any API
	rustdoc::private_intra_doc_links,
)]
// `egui` returns a response on every operation, but we don't use them
#![allow(unused_results)]
// We need to pass a lot of state around, without an easy way to bundle it
#![allow(clippy::too_many_arguments)]

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
	zsw_panels::{Panel, PanelState, PanelStateImage, PanelStateImages, Panels},
	zsw_playlist::{Playlist, PlaylistImage},
	zsw_profiles::{Profile, Profiles},
	zsw_side_effect_macros::side_effect,
	zsw_util::{MightBlock, Rect},
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
#[derive(Debug)]
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
	#[must_use]
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
	/// # Blocking
	/// Lock tree:
	/// [`zsw_wgpu::SurfaceLock`] on `wgpu`
	/// - [`zsw_egui::PlatformLock`] on `egui`
	///   - [`zsw_profiles::ProfilesLock`] on `profiles`
	///     - [`zsw_playlist::PlaylistLock`] on `playlist`
	///       - [`zsw_panels::PanelsLock`] on `panels`
	#[side_effect(MightBlock)]
	pub async fn run<'wgpu, 'egui, 'playlist, 'panels, 'profiles>(
		&self,
		wgpu: &'wgpu Wgpu<'_>,
		egui: &'egui Egui,
		window: &Window,
		panels: &'panels Panels,
		playlist: &'playlist Playlist,
		profiles: &'profiles Profiles,
	) -> ! {
		// Create the inner data
		// DEADLOCK: Caller ensures we can lock it
		// TODO: Check if it's fine to call `wgpu.surface_size`
		let mut inner = {
			let surface_lock = wgpu.lock_surface().await.allow::<MightBlock>();
			Inner::new(wgpu.surface_size(&surface_lock))
		};

		loop {
			// Get the surface size
			// DEADLOCK: Caller ensures we can lock it
			let surface_size = {
				let surface_lock = wgpu.lock_surface().await.allow::<MightBlock>();
				wgpu.surface_size(&surface_lock)
			};

			// Draw egui
			let res = {
				// DEADLOCK: Caller ensures we can lock it after the surface lock
				let mut platform_lock = egui.lock_platform().await.allow::<MightBlock>();


				// DEADLOCK: Caller ensures we can lock it after the platform lock
				let mut profiles_lock = profiles.lock_profiles().await.allow::<MightBlock>();

				// DEADLOCK: Caller ensures we can lock it after the profiles lock
				let mut playlist_lock = playlist.lock_playlist().await.allow::<MightBlock>();

				// DEADLOCK: Caller ensures we can lock it after the panels lock
				let mut panels_lock = panels.lock_panels().await.allow::<MightBlock>();

				egui.draw(window, &mut platform_lock, |ctx, frame| {
					self.draw(
						&mut inner,
						ctx,
						frame,
						surface_size,
						window,
						panels,
						playlist,
						profiles,
						&mut playlist_lock,
						&mut panels_lock,
						&mut profiles_lock,
					)
				})
			};

			let paint_jobs = match res {
				Ok(paint_jobs) => paint_jobs,
				Err(err) => {
					log::warn!("Unable to draw egui: {err:?}");
					continue;
				},
			};

			// Then send the paint jobs
			// DEADLOCK: We ensure we don't hold any locks while
			//           sending.
			self.paint_jobs_tx
				.send(paint_jobs)
				.await
				.expect("Paint jobs receiver was closed");
		}
	}

	/// Retrieves the paint jobs for the next frame
	///
	/// # Blocking
	/// Locks [`zsw_wgpu::SurfaceLock`] on `wgpu`
	// TODO: Replace with a barrier
	// Note: Doesn't literally lock it, but the other side of the channel
	//       needs to lock it in order to progress, so it's equivalent
	#[side_effect(MightBlock)]
	pub async fn paint_jobs<'wgpu>(&self, _wgpu: &'wgpu Wgpu<'_>) -> Vec<egui::epaint::ClippedMesh> {
		// Note: This can't return an `Err` because `self` owns a sender
		// DEADLOCK: Caller ensures it won't hold a `SurfaceLock`,
		//           and we ensure the other side of the channel
		//           can progress.
		self.paint_jobs_rx.recv().await.expect("Paint jobs sender was closed")
	}

	/// Draws the settings window
	fn draw<'playlist, 'panels, 'profiles>(
		&self,
		inner: &mut Inner,
		ctx: &egui::CtxRef,
		_frame: &epi::Frame,
		surface_size: PhysicalSize<u32>,
		window: &Window,
		panels: &'panels Panels,
		playlist: &'playlist Playlist,
		profiles: &'profiles Profiles,
		playlist_lock: &mut zsw_playlist::PlaylistLock<'playlist>,
		panels_lock: &mut zsw_panels::PanelsLock<'panels>,
		profiles_lock: &mut zsw_profiles::ProfilesLock<'profiles>,
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
			self::draw_settings_window(
				ui,
				&mut inner.new_panel_state,
				surface_size,
				panels,
				playlist,
				profiles,
				playlist_lock,
				panels_lock,
				profiles_lock,
			);
		});

		Ok(())
	}

	/// Queues an open click
	pub fn queue_open_click(&self, cursor_pos: Option<PhysicalPosition<f64>>) {
		self.queued_open_click.store(cursor_pos);
	}
}

impl Default for SettingsWindow {
	fn default() -> Self {
		Self::new()
	}
}

/// Draws the settings window
fn draw_settings_window<'playlist, 'panels, 'profiles>(
	ui: &mut egui::Ui,
	new_panel_state: &mut NewPanelState,
	surface_size: PhysicalSize<u32>,
	panels: &'panels Panels,
	playlist: &'playlist Playlist,
	profiles: &'profiles Profiles,
	playlist_lock: &mut zsw_playlist::PlaylistLock<'playlist>,
	panels_lock: &mut zsw_panels::PanelsLock<'panels>,
	profiles_lock: &mut zsw_profiles::ProfilesLock<'profiles>,
) {
	// Draw the panels header
	ui.collapsing("Panels", |ui| {
		self::draw_panels(ui, new_panel_state, surface_size, panels, panels_lock);
	});
	ui.collapsing("Playlist", |ui| {
		self::draw_playlist(ui, playlist, playlist_lock);
	});
	ui.collapsing("Profile", |ui| {
		self::draw_profile(
			ui,
			panels,
			playlist,
			profiles,
			playlist_lock,
			panels_lock,
			profiles_lock,
		);
	});
}

/// Draws the profile settings
fn draw_profile<'playlist, 'panels, 'profiles>(
	ui: &mut egui::Ui,
	panels: &'panels Panels,
	playlist: &'playlist Playlist,
	profiles: &'profiles Profiles,
	playlist_lock: &mut zsw_playlist::PlaylistLock<'playlist>,
	panels_lock: &mut zsw_panels::PanelsLock<'panels>,
	profiles_lock: &mut zsw_profiles::ProfilesLock<'profiles>,
) {
	// Draw all profiles
	for (path, profile) in profiles.profiles(profiles_lock) {
		ui.horizontal(|ui| {
			ui.label(path.display().to_string());
			if ui.button("Apply").clicked() {
				profile.apply(playlist, panels, playlist_lock, panels_lock).block_on();
			}
		});
	}

	// Draw the load button
	ui.horizontal(|ui| {
		ui.label("Load");
		if ui.button("📁").clicked() {
			let file_dialog = native_dialog::FileDialog::new().show_open_single_file();
			match file_dialog {
				Ok(file_dialog) =>
					if let Some(path) = file_dialog {
						match profiles.load(profiles_lock, path.clone()) {
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
						let profile = {
							Profile {
								root_path: match playlist.root_path(playlist_lock).block_on() {
									Some(path) => path,
									None => {
										log::warn!("No root path was set");
										return;
									},
								},
								panels:    panels.panels(panels_lock).iter().map(|panel| panel.panel).collect(),
							}
						};

						match profiles.save(profiles_lock, path.clone(), profile) {
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
fn draw_playlist<'playlist>(
	ui: &mut egui::Ui,
	playlist: &'playlist Playlist,
	playlist_lock: &mut zsw_playlist::PlaylistLock<'playlist>,
) {
	// Draw the root path
	ui.horizontal(|ui| {
		// Show the current root path
		ui.label("Root path");
		ui.add_space(10.0);
		{
			match playlist.root_path(playlist_lock).block_on() {
				Some(root_path) => ui.label(root_path.display().to_string()),
				None => ui.label("<None>"),
			};
		}

		// Then the change button
		if ui.button("📁").clicked() {
			// Ask for a file
			let file_dialog = native_dialog::FileDialog::new().show_open_single_dir();
			match file_dialog {
				Ok(file_dialog) => {
					if let Some(path) = file_dialog {
						playlist.set_root_path(playlist_lock, path).block_on();

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
				.peek_next(playlist_lock, |image| match image {
					PlaylistImage::File(path) => {
						ui.label(path.display().to_string());
					},
				})
				.block_on();
		});
	});
}

/// Draws the panels settings
fn draw_panels<'panels>(
	ui: &mut egui::Ui,
	new_panel_state: &mut NewPanelState,
	surface_size: PhysicalSize<u32>,
	panels: &'panels Panels,
	panels_lock: &mut zsw_panels::PanelsLock<'panels>,
) {
	// Draw all panels in their own header
	for (idx, panel) in panels.panels_mut(panels_lock).iter_mut().enumerate() {
		ui.collapsing(format!("Panel {idx}"), |ui| {
			ui.add(PanelWidget::new(panel, surface_size));
		});
	}

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
			panels.add_panel(
				panels_lock,
				Panel::new(
					new_panel_state.geometry,
					new_panel_state.duration,
					new_panel_state.fade_point,
				),
			);
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

		ui.collapsing("Images", |ui| {
			match &mut self.panel.images {
				PanelStateImages::Empty => (),
				PanelStateImages::PrimaryOnly { front } => self::draw_panel_state_images(ui, "Front", front),
				PanelStateImages::Both { front, back } => {
					self::draw_panel_state_images(ui, "Front", front);
					ui.separator();
					self::draw_panel_state_images(ui, "Back", back);
				},
			};
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

fn draw_panel_state_images(ui: &mut egui::Ui, kind: &str, image: &mut PanelStateImage) {
	ui.horizontal(|ui| {
		ui.label(kind);
		ui.label(image.image.image_path().display().to_string());
	});
	ui.horizontal(|ui| {
		ui.checkbox(&mut image.swap_dir, "Swap direction");
	});
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