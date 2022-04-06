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
	// This is too prevalent on generic functions, which we don't want to ALWAYS be `Send`
	clippy::future_not_send,
)]
// `egui` returns a response on every operation, but we don't use them
#![allow(unused_results)]
// We need to pass a lot of state around, without an easy way to bundle it
#![allow(clippy::too_many_arguments)]

// Imports
use {
	cgmath::{Point2, Vector2},
	egui::{plot, Widget},
	futures::lock::Mutex,
	pollster::FutureExt,
	winit::{
		dpi::{PhysicalPosition, PhysicalSize},
		window::Window,
	},
	zsw_egui::Egui,
	zsw_panels::{Panel, PanelState, PanelStateImage, PanelStateImages, Panels, PanelsResource},
	zsw_playlist::{Playlist, PlaylistImage, PlaylistResource},
	zsw_profiles::{Profile, Profiles, ProfilesResource},
	zsw_util::{Rect, ResourcesLock, ServicesContains},
	zsw_wgpu::{Wgpu, WgpuSurfaceResource},
};

/// Inner data
struct Inner {
	/// If open
	open: bool,

	/// New panel state
	new_panel_state: NewPanelState,

	/// Queued open click
	queued_open_click: Option<PhysicalPosition<f64>>,
}

impl Inner {
	/// Creates the inner data
	pub fn new(surface_size: PhysicalSize<u32>) -> Self {
		Self {
			open:              false,
			new_panel_state:   NewPanelState::new(surface_size),
			queued_open_click: None,
		}
	}
}

/// Settings window
#[derive(Debug)]
pub struct SettingsWindow {
	/// Inner
	inner: Mutex<Inner>,
}

impl SettingsWindow {
	/// Creates the settings window
	#[must_use]
	pub fn new(window: &Window) -> Self {
		// Create the inner data
		// TODO: Check if it's fine to use the window size here instead of the
		//       wgpu surface size
		let window_size = window.inner_size();
		let inner = Inner::new(window_size);

		Self {
			inner: Mutex::new(inner),
		}
	}

	/// Runs the setting window
	///
	/// # Blocking
	/// Lock tree:
	/// [`zsw_wgpu::SurfaceLock`] on `wgpu`
	/// [`zsw_egui::PlatformLock`] on `egui`
	/// - [`zsw_profiles::ProfilesLock`] on `profiles`
	///   - [`zsw_playlist::PlaylistLock`] on `playlist`
	///     - [`zsw_panels::PanelsLock`] on `panels`
	/// Blocks until [`Self::paint_jobs`] on `egui` is called.
	pub async fn run<S, R>(&self, services: &S, resources: &R) -> !
	where
		S: ServicesContains<Wgpu>
			+ ServicesContains<Egui>
			+ ServicesContains<Window>
			+ ServicesContains<Panels>
			+ ServicesContains<Playlist>
			+ ServicesContains<Profiles>,
		R: ResourcesLock<PanelsResource>
			+ ResourcesLock<PlaylistResource>
			+ ResourcesLock<ProfilesResource>
			+ ResourcesLock<WgpuSurfaceResource>,
	{
		let wgpu = services.service::<Wgpu>();
		let egui = services.service::<Egui>();
		let profiles = services.service::<Profiles>();
		let playlist = services.service::<Playlist>();
		let window = services.service::<Window>();
		let panels = services.service::<Panels>();


		loop {
			// Get the surface size
			// DEADLOCK: Caller ensures we can lock it
			let surface_size = {
				let surface_lock = resources.resource::<WgpuSurfaceResource>().await;
				wgpu.surface_size(&surface_lock)
			};

			// Draw egui
			let res = {
				// DEADLOCK: Caller ensures we can lock it
				let mut platform_lock = egui.lock_platform().await;


				// DEADLOCK: Caller ensures we can lock it after the platform lock
				let mut profiles_resource = resources.resource::<ProfilesResource>().await;

				// DEADLOCK: Caller ensures we can lock it after the profiles lock
				let mut playlist_resource = resources.resource::<PlaylistResource>().await;

				// DEADLOCK: Caller ensures we can lock it after the panels lock
				let mut panels_resource = resources.resource::<PanelsResource>().await;

				// TODO: Check locking of this
				let mut inner = self.inner.lock().await;

				egui.draw(window, &mut platform_lock, |ctx, frame| {
					Self::draw(
						&mut inner,
						ctx,
						frame,
						surface_size,
						window,
						panels,
						playlist,
						profiles,
						&mut playlist_resource,
						&mut panels_resource,
						&mut profiles_resource,
					)
				})
			};

			match res {
				// DEADLOCK: Caller ensures we can block
				Ok(paint_jobs) => egui.update_paint_jobs(paint_jobs).await,
				Err(err) => log::warn!("Unable to draw egui: {err:?}"),
			}
		}
	}

	/// Draws the settings window
	fn draw<'playlist, 'panels, 'profiles>(
		inner: &mut Inner,
		ctx: &egui::CtxRef,
		_frame: &epi::Frame,
		surface_size: PhysicalSize<u32>,
		window: &Window,
		panels: &'panels Panels,
		playlist: &'playlist Playlist,
		profiles: &'profiles Profiles,
		playlist_resource: &mut PlaylistResource,
		panels_resource: &mut PanelsResource,
		profiles_resource: &mut ProfilesResource,
	) -> Result<(), anyhow::Error> {
		// Create the base settings window
		let mut settings_window = egui::Window::new("Settings");

		// If we have any queued click, summon the window there
		if let Some(cursor_pos) = inner.queued_open_click.take() {
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
				playlist_resource,
				panels_resource,
				profiles_resource,
			);
		});

		Ok(())
	}

	/// Queues an open click
	// TODO: Maybe move this to input system?
	pub async fn queue_open_click(&self, cursor_pos: Option<PhysicalPosition<f64>>) {
		self.inner.lock().await.queued_open_click = cursor_pos;
	}

	/// Returns if the window is open
	pub async fn is_open(&self) -> bool {
		self.inner.lock().await.open
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
	playlist_resource: &mut PlaylistResource,
	panels_resource: &mut PanelsResource,
	profiles_resource: &mut ProfilesResource,
) {
	// Draw the panels header
	ui.collapsing("Panels", |ui| {
		self::draw_panels(ui, new_panel_state, surface_size, panels, panels_resource);
	});
	ui.collapsing("Playlist", |ui| {
		self::draw_playlist(ui, playlist, playlist_resource);
	});
	ui.collapsing("Profile", |ui| {
		self::draw_profile(
			ui,
			panels,
			playlist,
			profiles,
			playlist_resource,
			panels_resource,
			profiles_resource,
		);
	});
}

/// Draws the profile settings
fn draw_profile<'playlist, 'panels, 'profiles>(
	ui: &mut egui::Ui,
	panels: &'panels Panels,
	playlist: &'playlist Playlist,
	profiles: &'profiles Profiles,
	playlist_resource: &mut PlaylistResource,
	panels_resource: &mut PanelsResource,
	profiles_resource: &mut ProfilesResource,
) {
	// Draw all profiles
	for (path, profile) in profiles.profiles(profiles_resource) {
		ui.horizontal(|ui| {
			ui.label(path.display().to_string());
			if ui.button("Apply").clicked() {
				profile
					.apply(playlist, panels, playlist_resource, panels_resource)
					.block_on();
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
						match profiles.load(profiles_resource, path.clone()) {
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
								root_path: match playlist.root_path(playlist_resource).block_on() {
									Some(path) => path,
									None => {
										log::warn!("No root path was set");
										return;
									},
								},
								panels:    panels.panels(panels_resource).iter().map(|panel| panel.panel).collect(),
							}
						};

						match profiles.save(profiles_resource, path.clone(), profile) {
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
	playlist_resource: &mut PlaylistResource,
) {
	// Draw the root path
	ui.horizontal(|ui| {
		// Show the current root path
		ui.label("Root path");
		ui.add_space(10.0);
		{
			match playlist.root_path(playlist_resource).block_on() {
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
						playlist.set_root_path(playlist_resource, path).block_on();

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
				.peek_next(playlist_resource, |image| match image {
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
	panels_resource: &mut PanelsResource,
) {
	// Draw all panels in their own header
	for (idx, panel) in panels.panels_mut(panels_resource).iter_mut().enumerate() {
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
				panels_resource,
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

		ui.horizontal(|ui| {
			ui.label("Parallax ratio");
			egui::Slider::new(&mut self.panel.panel.parallax_ratio, 0.0..=1.0).ui(ui);
		});

		ui.vertical(|ui| {
			ui.horizontal(|ui| {
				ui.label("Parallax exp");
				egui::Slider::new(&mut self.panel.panel.parallax_exp, 0.0..=4.0).ui(ui);
			});

			ui.collapsing("Graph", |ui| {
				let it = (0..=100).map(|i| {
					let x = i as f32 / 100.0;
					plot::Value::new(x, x.signum() * x.abs().powf(self.panel.panel.parallax_exp))
				});
				let line = plot::Line::new(plot::Values::from_values_iter(it));

				plot::Plot::new("Frame timings (ms)")
					.allow_drag(false)
					.allow_zoom(false)
					.show_background(false)
					.view_aspect(1.0)
					.show(ui, |plot_ui| plot_ui.line(line));
			});
		});


		ui.horizontal(|ui| {
			ui.checkbox(&mut self.panel.panel.reverse_parallax, "Reverse parallax");
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
