//! Settings window

// Features
#![feature(never_type, let_chains)]
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
	std::{mem, sync::Arc},
	winit::{dpi::PhysicalSize, window::Window},
	zsw_egui::EguiPainter,
	zsw_input::InputReceiver,
	zsw_panels::{
		Panel,
		PanelImage,
		PanelState,
		PanelStateImageState,
		PanelStateImagesState,
		PanelsEditor,
		PanelsResource,
		PanelsShader,
	},
	zsw_playlist::{PlaylistImage, PlaylistManager},
	zsw_profiles::{Profile, ProfilesManager},
	zsw_util::{Rect, Resources},
	zsw_wgpu::{Wgpu, WgpuSurfaceResource},
};

/// Settings window state
#[derive(Debug)]
struct SettingsWindowState {
	/// If open
	open: bool,

	/// New panel state
	new_panel_state: NewPanelState,
}

/// Settings window
#[derive(Debug)]
pub struct SettingsWindow<P> {
	/// Egui painter
	egui_painter: EguiPainter,

	/// Input receiver
	input_receiver: InputReceiver,

	/// Profile applier
	profile_applier: P,

	/// Wgpu
	wgpu: Wgpu,

	/// Window
	window: Arc<Window>,

	/// Panels editor
	panels_editor: PanelsEditor,

	/// Playlist manager
	playlist_manager: PlaylistManager,

	/// Profiles manager
	profiles_manager: ProfilesManager,

	/// State
	state: SettingsWindowState,
}

impl<P> SettingsWindow<P> {
	/// Creates the settings window
	#[must_use]
	pub fn new(
		window: Arc<Window>,
		egui_painter: EguiPainter,
		input_receiver: InputReceiver,
		profile_applier: P,
		wgpu: Wgpu,
		panels_editor: PanelsEditor,
		playlist_manager: PlaylistManager,
		profiles_manager: ProfilesManager,
	) -> Self {
		let state = SettingsWindowState {
			open:            false,
			// TODO: Check if it's fine to use the window size here instead of the
			//       wgpu surface size
			new_panel_state: NewPanelState::new(window.inner_size()),
		};
		Self {
			egui_painter,
			input_receiver,
			profile_applier,
			wgpu,
			window,
			panels_editor,
			playlist_manager,
			profiles_manager,
			state,
		}
	}

	/// Runs the setting window
	pub async fn run<R>(&mut self, resources: &mut R)
	where
		R: Resources<PanelsResource> + Resources<WgpuSurfaceResource>,
		P: ProfileApplier,
	{
		loop {
			// Get the surface size
			let surface_size = self
				.wgpu
				.surface_size(&*resources.resource::<WgpuSurfaceResource>().await);
			let mut panels_resource = resources.resource::<PanelsResource>().await;

			// Draw
			let res = self
				.egui_painter
				.draw(&self.window, |ctx, frame| {
					Self::draw(
						&mut self.state,
						&mut self.input_receiver,
						&mut self.profile_applier,
						&mut self.playlist_manager,
						&mut self.profiles_manager,
						&mut self.panels_editor,
						&self.window,
						ctx,
						frame,
						surface_size,
						&mut panels_resource,
					);
					mem::drop(panels_resource);
				})
				.await;

			// If the renderer has quit, quit too
			if res.is_none() {
				tracing::debug!("Quitting settings window: Receiver quit");
				break;
			}
		}
	}

	/// Draws the settings window
	fn draw(
		state: &mut SettingsWindowState,
		input_receiver: &mut InputReceiver,
		profile_applier: &mut P,
		playlist_manager: &mut PlaylistManager,
		profiles_manager: &mut ProfilesManager,
		panels_editor: &mut PanelsEditor,
		window: &Window,
		ctx: &egui::Context,
		_frame: &epi::Frame,
		surface_size: PhysicalSize<u32>,
		panels_resource: &mut PanelsResource,
	) where
		P: ProfileApplier,
	{
		// Create the base settings window
		let mut settings_window = egui::Window::new("Settings");

		// If we got a click, open the window
		if input_receiver.on_click() == Some(winit::event::MouseButton::Right) && let Some(cursor_pos) = input_receiver.cursor_pos() {
			tracing::debug!("Opening settings window");

			// Adjust cursor pos to account for the scale factor
			let scale_factor = window.scale_factor();
			let cursor_pos = cursor_pos.to_logical(scale_factor);

			// Then set the current position and that we're open
			settings_window = settings_window.current_pos(egui::pos2(cursor_pos.x, cursor_pos.y));
			state.open = true;
		}

		// Then render it
		settings_window.open(&mut state.open).show(ctx, |ui| {
			self::draw_settings_window(
				ui,
				&mut state.new_panel_state,
				surface_size,
				panels_resource,
				profile_applier,
				playlist_manager,
				profiles_manager,
				panels_editor,
			);
		});
	}
}

/// Draws the settings window
fn draw_settings_window(
	ui: &mut egui::Ui,
	new_panel_state: &mut NewPanelState,
	surface_size: PhysicalSize<u32>,
	panels_resource: &mut PanelsResource,
	profile_applier: &mut impl ProfileApplier,
	playlist_manager: &mut PlaylistManager,
	profiles_manager: &mut ProfilesManager,
	panels_editor: &mut PanelsEditor,
) {
	// Draw the panels header
	ui.collapsing("Panels", |ui| {
		self::draw_panels(ui, new_panel_state, surface_size, panels_resource, panels_editor);
	});
	ui.collapsing("Playlist", |ui| {
		self::draw_playlist(ui, playlist_manager);
	});
	ui.collapsing("Profile", |ui| {
		self::draw_profile(ui, panels_resource, profile_applier, profiles_manager);
	});
}

/// Draws the profile settings
fn draw_profile(
	ui: &mut egui::Ui,
	panels_resource: &mut PanelsResource,
	profile_applier: &mut impl ProfileApplier,
	profiles_manager: &mut ProfilesManager,
) {
	// Draw all profiles
	for (path, profile) in profiles_manager.profiles() {
		ui.horizontal(|ui| {
			ui.label(path.display().to_string());
			if ui.button("Apply").clicked() {
				profile_applier.apply(&profile, panels_resource);
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
						let profile = profile_applier.current(panels_resource);
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
fn draw_playlist(ui: &mut egui::Ui, playlist_manager: &mut PlaylistManager) {
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
fn draw_panels(
	ui: &mut egui::Ui,
	new_panel_state: &mut NewPanelState,
	surface_size: PhysicalSize<u32>,
	panels_resource: &mut PanelsResource,
	panels_editor: &mut PanelsEditor,
) {
	// TODO: Decide on number to put here?
	match panels_editor.max_image_size_mut(panels_resource) {
		Some(max_image_size) => {
			egui::Slider::new(max_image_size, 0..=8192).ui(ui);
		},
		None =>
			if ui.button("Add max image size").clicked() {
				// TODO: What value to default to here?
				panels_editor.set_max_image_size(panels_resource, Some(4096));
			},
	}

	ui.vertical(|ui| {
		ui.label("Shader");
		let cur_shader = panels_editor.shader_mut(panels_resource);
		egui::ComboBox::from_id_source(std::ptr::addr_of!(cur_shader))
			.selected_text(cur_shader.name())
			.show_ui(ui, |ui| {
				// TODO: Not have default values here?
				let shaders = [
					PanelsShader::Fade,
					PanelsShader::FadeWhite { strength: 1.0 },
					PanelsShader::FadeOut { strength: 0.2 },
					PanelsShader::FadeIn { strength: 0.2 },
				];
				for shader in shaders {
					ui.selectable_value(cur_shader, shader, shader.name());
				}
			});

		match cur_shader {
			PanelsShader::Fade => (),
			PanelsShader::FadeWhite { strength } => {
				ui.horizontal(|ui| {
					ui.label("Strength");
					egui::Slider::new(strength, 0.0..=20.0).ui(ui);
				});
			},
			PanelsShader::FadeOut { strength } => {
				ui.horizontal(|ui| {
					ui.label("Strength");
					egui::Slider::new(strength, 0.0..=2.0).ui(ui);
				});
			},
			PanelsShader::FadeIn { strength } => {
				ui.horizontal(|ui| {
					ui.label("Strength");
					egui::Slider::new(strength, 0.0..=2.0).ui(ui);
				});
			},
		}
	});

	// Draw all panels in their own header
	for (idx, panel) in panels_editor.panels_mut(panels_resource).iter_mut().enumerate() {
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
			panels_editor.add_panel(
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
#[derive(Debug)]
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
				PanelStateImagesState::Empty => (),
				PanelStateImagesState::PrimaryOnly { front } =>
					self::draw_panel_state_images(ui, "Front", front, &mut self.panel.front_image),
				PanelStateImagesState::Both { front, back } => {
					self::draw_panel_state_images(ui, "Front", front, &mut self.panel.front_image);
					ui.separator();
					self::draw_panel_state_images(ui, "Back", back, &mut self.panel.back_image);
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

fn draw_panel_state_images(
	ui: &mut egui::Ui,
	kind: &str,
	image_state: &mut PanelStateImageState,
	image: &mut PanelImage,
) {
	ui.horizontal(|ui| {
		ui.label(kind);
		ui.label(&image.name);
	});
	ui.horizontal(|ui| {
		ui.checkbox(&mut image_state.swap_dir, "Swap direction");
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
// TODO: Since it also gets the current profile, it's no longer just an applier, rename?
//       `ProfileManager` might be too general though.
pub trait ProfileApplier {
	/// Applies a profile
	// TODO: Not hardcore `panels_resource` once we remove resources?
	fn apply(&mut self, profile: &Profile, panels_resource: &mut PanelsResource);

	/// Retrieves the current profile
	// TODO: Same TODO as above
	fn current(&mut self, panels_resource: &mut PanelsResource) -> Profile;
}
