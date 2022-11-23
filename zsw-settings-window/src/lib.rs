//! Settings window

// Features
#![feature(never_type)]
// `egui` returns a response on every operation, but we don't use them
#![allow(unused_results)]
// We need to pass a lot of state around, without an easy way to bundle it
#![allow(clippy::too_many_arguments)]
// TODO: Split into smaller functions
#![allow(clippy::cognitive_complexity)]

// Imports
use {
	cgmath::{Point2, Vector2},
	egui::Widget,
	futures::lock::Mutex,
	winit::{
		dpi::{PhysicalPosition, PhysicalSize},
		window::Window,
	},
	zsw_egui::{Egui, EguiPainterResource, EguiPlatformResource},
	zsw_panels::{Panel, PanelState, PanelStateImage, PanelStateImages, Panels, PanelsResource},
	zsw_playlist::{PlaylistImage, PlaylistManager},
	zsw_profiles::{Profile, ProfilesManager},
	zsw_util::{Rect, Resources, Services, ServicesBundle},
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
	/// Blocks until [`Self::update_output`] on `egui` is called.
	pub async fn run<S, R>(
		&self,
		services: &S,
		resources: &R,
		egui_painter_resource: &mut EguiPainterResource,
		profile_applier: impl ProfileApplier<S>,
	) -> !
	where
		S: Services<Wgpu>
			+ Services<Egui>
			+ Services<Window>
			+ Services<Panels>
			+ Services<PlaylistManager>
			+ Services<ProfilesManager>,
		R: Resources<PanelsResource> + Resources<WgpuSurfaceResource> + Resources<EguiPlatformResource>,
	{
		let wgpu = services.service::<Wgpu>();
		let egui = services.service::<Egui>();
		let window = services.service::<Window>();


		loop {
			// Get the surface size
			// DEADLOCK: Caller ensures we can lock it
			let surface_size = {
				let wgpu_surface_resource = resources.resource::<WgpuSurfaceResource>().await;
				wgpu.surface_size(&wgpu_surface_resource)
			};

			// Draw egui
			let res = {
				// DEADLOCK: Caller ensures we can lock it
				let mut egui_platform_resource = resources.resource::<EguiPlatformResource>().await;

				// DEADLOCK: Caller ensures we can lock it after the panels lock
				let mut panels_resource = resources.resource::<PanelsResource>().await;

				// TODO: Check locking of this
				let mut inner = self.inner.lock().await;

				egui.draw(window, &mut egui_platform_resource, |ctx, frame| {
					Self::draw(
						&mut inner,
						ctx,
						frame,
						surface_size,
						services,
						&mut panels_resource,
						&profile_applier,
					)
				})
			};

			// Try to update the output
			match res {
				Ok(output) => egui.update_output(egui_painter_resource, output).await,
				Err(err) => tracing::warn!(?err, "Unable to draw egui"),
			}
		}
	}

	/// Draws the settings window
	fn draw<S>(
		inner: &mut Inner,
		ctx: &egui::Context,
		_frame: &epi::Frame,
		surface_size: PhysicalSize<u32>,
		services: &S,
		panels_resource: &mut PanelsResource,
		profile_applier: &impl ProfileApplier<S>,
	) -> Result<(), anyhow::Error>
	where
		S: Services<Window> + Services<Panels> + Services<PlaylistManager> + Services<ProfilesManager>,
	{
		let window = services.service::<Window>();

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
				services,
				panels_resource,
				profile_applier,
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
fn draw_settings_window<S>(
	ui: &mut egui::Ui,
	new_panel_state: &mut NewPanelState,
	surface_size: PhysicalSize<u32>,
	services: &S,
	panels_resource: &mut PanelsResource,
	profile_applier: &impl ProfileApplier<S>,
) where
	S: Services<Panels> + Services<PlaylistManager> + Services<ProfilesManager>,
{
	// Draw the panels header
	ui.collapsing("Panels", |ui| {
		self::draw_panels(ui, new_panel_state, surface_size, services, panels_resource);
	});
	ui.collapsing("Playlist", |ui| {
		self::draw_playlist(ui, services);
	});
	ui.collapsing("Profile", |ui| {
		self::draw_profile(ui, services, panels_resource, profile_applier);
	});
}

/// Draws the profile settings
fn draw_profile<S>(
	ui: &mut egui::Ui,
	services: &S,
	panels_resource: &mut PanelsResource,
	profile_applier: &impl ProfileApplier<S>,
) where
	S: Services<Panels> + Services<PlaylistManager> + Services<ProfilesManager>,
{
	let panels = services.service::<Panels>();
	let playlist_manager = services.service::<PlaylistManager>();
	let profiles_manager = services.service::<ProfilesManager>();

	// Draw all profiles
	for (path, profile) in profiles_manager.profiles() {
		ui.horizontal(|ui| {
			ui.label(path.display().to_string());
			if ui.button("Apply").clicked() {
				profile_applier.apply(&profile, services, panels_resource);
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
						match profiles_manager.load(path.clone()) {
							Ok(profile) => tracing::info!(?profile, "Successfully loaded profile"),
							Err(err) => tracing::warn!(?path, ?err, "Unable to load profile"),
						}
					},
				Err(err) => tracing::warn!(?err, "Unable to ask user for new root directory"),
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
								root_path: match playlist_manager.root_path() {
									Some(path) => path,
									None => {
										tracing::warn!("No root path was set");
										return;
									},
								},
								panels:    panels.panels(panels_resource).iter().map(|panel| panel.panel).collect(),
							}
						};

						if let Err(err) = profiles_manager.save(path.clone(), profile) {
							tracing::warn!(?path, ?err, "Unable to load profile");
						}
					},
				Err(err) => tracing::warn!(?err, "Unable to ask user for new root directory"),
			}
		}
	});
}

/// Draws the playlist settings
fn draw_playlist<S>(ui: &mut egui::Ui, services: &S)
where
	S: Services<PlaylistManager>,
{
	let playlist_manager = services.service::<PlaylistManager>();

	// Draw the root path
	ui.horizontal(|ui| {
		// Show the current root path
		ui.label("Root path");
		ui.add_space(10.0);
		{
			match playlist_manager.root_path() {
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
						playlist_manager.set_root_path(path);

						// TODO: Maybe reset both panels and loaders?
					}
				},
				Err(err) => tracing::warn!("Unable to ask user for new root directory: {err:?}"),
			}
		}
	});

	// Draw all paths in the pipeline
	ui.collapsing("Upcoming", |ui| {
		egui::ScrollArea::new([true, true]).max_height(500.0).show(ui, |ui| {
			for image in playlist_manager.peek_next() {
				match &*image {
					PlaylistImage::File(path) => {
						ui.label(path.display().to_string());
					},
				}
			}
		});
	});
}

/// Draws the panels settings
fn draw_panels<S>(
	ui: &mut egui::Ui,
	new_panel_state: &mut NewPanelState,
	surface_size: PhysicalSize<u32>,
	services: &S,
	panels_resource: &mut PanelsResource,
) where
	S: Services<Panels>,
{
	let panels = services.service::<Panels>();

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

		ui.horizontal(|ui| {
			ui.label("Parallax exp");
			egui::Slider::new(&mut self.panel.panel.parallax_exp, 0.0..=4.0).ui(ui);
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
		ui.label(image.image.image_name());
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

/// Profile applier
// TODO: Move this elsewhere once used elsewhere?
pub trait ProfileApplier<S: ServicesBundle> {
	/// Applies a profile
	// TODO: Not hardcore `panels_resource` once we remove resources?
	// TODO: Not pass `services` and have `self` store them instead?
	fn apply(&self, profile: &Profile, services: &S, panels_resource: &mut PanelsResource);
}
