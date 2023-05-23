//! Settings menu

// Lints
#![allow(unused_results)] // Egui produces a lot of results we don't need to use
#![expect(clippy::too_many_lines)] // TODO: Refactor

// Imports
use {
	crate::{
		panel::{self, PanelImage, PanelShader},
		playlist::PlaylistItemKind,
		shared::{AsyncLocker, AsyncMutexResource, AsyncRwLockResource, Shared},
	},
	anyhow::Context,
	egui::Widget,
	std::{
		path::{Path, PathBuf},
		sync::Arc,
	},
	zsw_util::{Rect, TokioTaskBlockOn},
};

/// Settings menu
#[derive(Debug)]
pub struct SettingsMenu {
	/// If open
	open: bool,

	/// Current tab
	cur_tab: Tab,

	/// Add playlist state
	add_playlist_state: AddPlaylistState,
}

impl SettingsMenu {
	/// Creates the settings menu
	pub fn new() -> Self {
		Self {
			open:               false,
			cur_tab:            Tab::Panels,
			add_playlist_state: AddPlaylistState::default(),
		}
	}

	/// Draws the settings menu
	pub fn draw(&mut self, ctx: &egui::Context, shared: &Arc<Shared>, locker: &mut AsyncLocker<'_, 0>) {
		// Adjust cursor pos to account for the scale factor
		let scale_factor = shared.window.scale_factor();
		let cursor_pos = shared.cursor_pos.load().cast::<f32>().to_logical(scale_factor);

		// Create the window
		let mut egui_window = egui::Window::new("Settings");

		// Open it at the mouse if pressed
		if !ctx.is_pointer_over_area() &&
			ctx.input(|input| input.pointer.button_clicked(egui::PointerButton::Secondary))
		{
			egui_window = egui_window.current_pos(egui::pos2(cursor_pos.x, cursor_pos.y));
			self.open = true;
		}

		// Then render it
		egui_window.open(&mut self.open).show(ctx, |ui| {
			ui.horizontal(|ui| {
				ui.selectable_value(&mut self.cur_tab, Tab::Panels, "Panels");
				ui.selectable_value(&mut self.cur_tab, Tab::Playlists, "Playlists");
			});
			ui.separator();

			match self.cur_tab {
				Tab::Panels => self::draw_panels_tab(ui, shared, locker),
				Tab::Playlists => self::draw_playlists(&mut self.add_playlist_state, ui, shared, locker),
			}
		});
	}
}
/// Draws the panels tab
fn draw_panels_tab(ui: &mut egui::Ui, shared: &Arc<Shared>, locker: &mut AsyncLocker<'_, 0>) {
	self::draw_panels_editor(ui, shared, locker);
	ui.separator();
	self::draw_shader_select(ui, shared, locker);
}

/// Draws the playlists tab
fn draw_playlists(
	add_playlist_state: &mut AddPlaylistState,
	ui: &mut egui::Ui,
	shared: &Arc<Shared>,
	locker: &mut AsyncLocker<'_, 0>,
) {
	let playlists = shared
		.playlists_manager
		.get_all_loaded(&shared.playlists, locker)
		.block_on();

	for (playlist_path, playlist) in playlists {
		ui.collapsing(playlist_path.to_string_lossy(), |ui| match playlist {
			Some(Ok(playlist)) => {
				let items = playlist.read(locker).block_on().0.items();

				for item in items {
					let (mut item, _) = item.write(locker).block_on();

					ui.checkbox(&mut item.enabled, "Enabled");
					match &mut item.kind {
						PlaylistItemKind::Directory { path, recursive } => {
							ui.horizontal(|ui| {
								ui.label("Dir: ");
								self::draw_openable_path(ui, path);
							});

							ui.checkbox(recursive, "Recursive");
						},
						PlaylistItemKind::File { path } => {
							ui.horizontal(|ui| {
								ui.label("File: ");
								self::draw_openable_path(ui, path);
							});
						},
					}

					if ui.button("↻ (Reload)").clicked() {
						let playlist_path = Arc::clone(&playlist_path);
						let shared = Arc::clone(shared);
						crate::spawn_task(format!("Reload playlist {playlist_path:?}"), |mut locker| async move {
							shared
								.playlists_manager
								.reload(&playlist_path, &shared.playlists, &mut locker)
								.await
								.context("Unable to reload playlist")?;

							Ok(())
						});
					}

					if ui.button("💾 (Save)").clicked() {
						let playlist_path = Arc::clone(&playlist_path);
						let shared = Arc::clone(shared);
						crate::spawn_task(format!("Saving playlist {playlist_path:?}"), |mut locker| async move {
							shared
								.playlists_manager
								.save(&playlist_path, &shared.playlists, &mut locker)
								.await
								.context("Unable to save playlist")?;

							Ok(())
						});
					}

					ui.separator();
				}
			},
			Some(Err(err)) => {
				ui.label(format!("Error: {:?}", anyhow::anyhow!(err)));
			},
			None => {
				ui.label("Loading...");
			},
		});
	}

	if ui.button("➕ (Add playlist)").clicked() {
		// If we don't have a start directory for the playlists, try to get one
		if matches!(add_playlist_state.start_dir, AddPlaylistStateStartPath::None) {
			if let Some((playlist_path, _)) = shared
				.playlists_manager
				.get_loaded_any(&shared.playlists, locker)
				.block_on()
			{
				if let Some(playlist_dir) = playlist_path.parent() {
					add_playlist_state.start_dir =
						AddPlaylistStateStartPath::FromExistingPlaylists(playlist_dir.to_path_buf());
				}
			};
		};

		// TODO: Not have this yaml filter here? Or at least allow files other than `.yaml`
		let mut file_dialog = rfd::FileDialog::new().add_filter("Playlist file", &["yaml"]);

		// Set the starting directory, if we have any
		if let Some(playlist_dir) = add_playlist_state.start_dir.as_path() {
			file_dialog = file_dialog.set_directory(playlist_dir);
		}

		// Ask the user for a playlist file
		match file_dialog.pick_file() {
			// If we got it, try to load it
			Some(playlist_path) => {
				tracing::debug!(?playlist_path, "Adding playlist");

				// Set the playlist state to the parent file
				if let Some(path) = playlist_path.parent() {
					add_playlist_state.start_dir = AddPlaylistStateStartPath::LastPlaylist(path.to_path_buf());
				}

				match shared
					.playlists_manager
					.load(&playlist_path, &shared.playlists, locker)
					.block_on()
				{
					Ok(playlist) => tracing::debug!(?playlist_path, ?playlist, "Successfully added playlist"),
					Err(err) => tracing::warn!(?playlist_path, ?err, "Unable to add playlist"),
				}
			},

			// Else just log that the user cancelled it
			None => tracing::debug!("User cancelled add playlist"),
		}
	}
}

/// Draws the panels editor
// TODO: Not edit the values as-is, as that breaks some invariants of panels (such as duration versus image states)
fn draw_panels_editor(ui: &mut egui::Ui, shared: &Shared, locker: &mut AsyncLocker<'_, 0>) {
	let (mut panel_group, mut locker) = shared.cur_panel_group.lock(locker).block_on();
	match &mut *panel_group {
		Some(panel_group) =>
			for (panel_idx, panel) in panel_group.panels_mut().iter_mut().enumerate() {
				ui.collapsing(format!("Panel {panel_idx}"), |ui| {
					ui.checkbox(&mut panel.state.paused, "Paused");

					ui.collapsing("Geometries", |ui| {
						for (geometry_idx, geometry) in panel.geometries.iter_mut().enumerate() {
							ui.horizontal(|ui| {
								ui.label(format!("#{}: ", geometry_idx + 1));
								self::draw_rect(ui, &mut geometry.geometry);
							});
						}
					});

					ui.horizontal(|ui| {
						// Note: We only allow up until the duration - 1 so that you don't get stuck
						//       skipping images when you hold it at the max value
						ui.label("Cur progress");
						egui::Slider::new(
							&mut panel.state.cur_progress,
							0..=panel.state.duration.saturating_sub(1),
						)
						.clamp_to_range(true)
						.ui(ui);

						// Then clamp to the current max
						// Note: We don't just use this max above so the slider doesn't jitter when the max changes
						let cur_max = match panel.images.state() {
							panel::ImagesState::Empty => 0,
							panel::ImagesState::PrimaryOnly => panel.state.fade_point,
							panel::ImagesState::Both => panel.state.duration,
						};
						panel.state.cur_progress = panel.state.cur_progress.clamp(0, cur_max);
					});

					ui.horizontal(|ui| {
						ui.label("Fade Point");
						let min = panel.state.duration / 2;
						let max = panel.state.duration.saturating_sub(1);
						egui::Slider::new(&mut panel.state.fade_point, min..=max).ui(ui);
					});

					ui.horizontal(|ui| {
						ui.label("Duration");
						egui::Slider::new(&mut panel.state.duration, 0..=10800).ui(ui);
					});

					ui.horizontal(|ui| {
						ui.label("Parallax ratio");
						egui::Slider::new(&mut panel.state.parallax.ratio, 0.0..=1.0).ui(ui);
					});

					ui.horizontal(|ui| {
						ui.label("Parallax exp");
						egui::Slider::new(&mut panel.state.parallax.exp, 0.0..=4.0).ui(ui);
					});


					ui.horizontal(|ui| {
						ui.checkbox(&mut panel.state.parallax.reverse, "Reverse parallax");
					});

					ui.horizontal(|ui| {
						ui.label("Skip");
						if ui.button("🔄").clicked() {
							match panel.images.state() {
								panel::ImagesState::Empty => (),
								panel::ImagesState::PrimaryOnly => panel.state.cur_progress = panel.state.fade_point,
								panel::ImagesState::Both => panel.state.cur_progress = panel.state.duration,
							}
						}
					});

					ui.collapsing("Images", |ui| {
						match panel.images.state() {
							panel::ImagesState::Empty => (),
							panel::ImagesState::PrimaryOnly => {
								ui.collapsing("Front", |ui| self::draw_panel_image(ui, panel.images.front_mut()));
							},
							panel::ImagesState::Both => {
								ui.collapsing("Front", |ui| self::draw_panel_image(ui, panel.images.front_mut()));
								ui.collapsing("Back", |ui| self::draw_panel_image(ui, panel.images.back_mut()));
							},
						};
					});

					#[expect(clippy::significant_drop_in_scrutinee)] // False positive, we're not locking anything
					ui.collapsing("Playlist player", |ui| {
						let (playlist_player, _) = panel.playlist_player.write(&mut locker).block_on();

						let row_height = ui.text_style_height(&egui::TextStyle::Body);

						ui.collapsing("Prev", |ui| {
							egui::ScrollArea::new([false, true])
								.auto_shrink([false, true])
								.stick_to_right(true)
								.max_height(row_height * 10.0)
								.show_rows(ui, row_height, playlist_player.prev_items().len(), |ui, idx| {
									for item in playlist_player.prev_items().take(idx.end).skip(idx.start) {
										self::draw_openable_path(ui, item);
									}
								});
						});

						ui.collapsing("Next", |ui| {
							egui::ScrollArea::new([false, true])
								.auto_shrink([false, true])
								.stick_to_right(true)
								.max_height(row_height * 10.0)
								.show_rows(ui, row_height, playlist_player.peek_next_items().len(), |ui, idx| {
									for item in playlist_player.peek_next_items().take(idx.end).skip(idx.start) {
										self::draw_openable_path(ui, item);
									}
								});
						});

						ui.collapsing("All", |ui| {
							egui::ScrollArea::new([false, true])
								.auto_shrink([false, true])
								.stick_to_right(true)
								.max_height(row_height * 10.0)
								.show_rows(ui, row_height, playlist_player.all_items().len(), |ui, idx| {
									for item in playlist_player.all_items().take(idx.end).skip(idx.start) {
										self::draw_openable_path(ui, item);
									}
								});
						});

						// TODO: Allow a "Go back" button. Or even a full playlist solution
					});
				});
			},
		None => {
			ui.label("None loaded");
		},
	}
}


/// Draws an openable path
fn draw_openable_path(ui: &mut egui::Ui, path: &Path) {
	ui.horizontal(|ui| {
		ui.label("Path: ");
		// TODO: Not use lossy conversion to display it?
		if ui.link(path.to_string_lossy()).clicked() {
			if let Err(err) = opener::open(path) {
				tracing::warn!(?path, ?err, "Unable to open file");
			}
		}
	});
}

/// Draws a panel image
fn draw_panel_image(ui: &mut egui::Ui, image: &mut PanelImage) {
	let size = image.size();
	if let Some(path) = image.path() {
		self::draw_openable_path(ui, path);
	}
	ui.label(format!("Size: {}x{}", size.x, size.y));
	ui.checkbox(image.swap_dir_mut(), "Swap direction");
}

/// Draws the shader select
fn draw_shader_select(ui: &mut egui::Ui, shared: &Shared, locker: &mut AsyncLocker<'_, 0>) {
	ui.label("Shader");

	let (mut panels_renderer_shader, _) = shared.panels_renderer_shader.write(locker).block_on();
	let cur_shader = &mut panels_renderer_shader.shader;
	egui::ComboBox::from_id_source("Shader selection menu")
		.selected_text(cur_shader.name())
		.show_ui(ui, |ui| {
			// TODO: Not have default values here?
			let shaders = [
				PanelShader::None,
				PanelShader::Fade,
				PanelShader::FadeWhite { strength: 1.0 },
				PanelShader::FadeOut { strength: 0.2 },
				PanelShader::FadeIn { strength: 0.2 },
			];
			for shader in shaders {
				ui.selectable_value(cur_shader, shader, shader.name());
			}
		});

	match cur_shader {
		PanelShader::None | PanelShader::Fade => (),
		PanelShader::FadeWhite { strength } => {
			ui.horizontal(|ui| {
				ui.label("Strength");
				egui::Slider::new(strength, 0.0..=20.0).ui(ui);
			});
		},
		PanelShader::FadeOut { strength } => {
			ui.horizontal(|ui| {
				ui.label("Strength");
				egui::Slider::new(strength, 0.0..=2.0).ui(ui);
			});
		},
		PanelShader::FadeIn { strength } => {
			ui.horizontal(|ui| {
				ui.label("Strength");
				egui::Slider::new(strength, 0.0..=2.0).ui(ui);
			});
		},
	}
}

/// Draws a geometry rectangle
fn draw_rect(ui: &mut egui::Ui, geometry: &mut Rect<i32, u32>) {
	ui.horizontal(|ui| {
		egui::DragValue::new(&mut geometry.size.x).speed(10).ui(ui);
		ui.label("x");
		egui::DragValue::new(&mut geometry.size.y).speed(10).ui(ui);
		ui.label("+");
		egui::DragValue::new(&mut geometry.pos.x).speed(10).ui(ui);
		ui.label("+");
		egui::DragValue::new(&mut geometry.pos.y).speed(10).ui(ui);
	});
}


/// Tab
#[derive(PartialEq, Debug)]
enum Tab {
	Panels,
	Playlists,
}

/// State for adding a playlist
#[derive(Clone, Default, Debug)]
struct AddPlaylistState {
	/// Start directory
	start_dir: AddPlaylistStateStartPath,
}

/// Start path for [`AddPlaylistState::start_path`]
#[derive(Clone, Default, Debug)]
enum AddPlaylistStateStartPath {
	/// There are no playlists and user hasn't picked any playlists yet
	#[default]
	None,

	/// User hasn't picked any playlists, but we took the parent path of
	/// one of the existing playlists
	FromExistingPlaylists(PathBuf),

	/// User has picked a playlist, and this is it's parent path.
	///
	/// This variant will be chosen regardless if the last playlist was
	/// successfully loaded or not.
	LastPlaylist(PathBuf),
}

impl AddPlaylistStateStartPath {
	/// Returns the inner path, if any
	pub fn as_path(&self) -> Option<&Path> {
		match self {
			Self::None => None,
			Self::FromExistingPlaylists(path) | Self::LastPlaylist(path) => Some(path),
		}
	}
}
