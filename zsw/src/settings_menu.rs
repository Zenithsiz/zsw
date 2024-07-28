//! Settings menu

// Lints
#![allow(unused_results)] // Egui produces a lot of results we don't need to use
#![expect(clippy::too_many_lines)] // TODO: Refactor

// Imports
use {
	crate::{
		panel::{PanelImage, PanelShader, PanelsManager},
		playlist::{Playlist, PlaylistItemKind, PlaylistName},
		shared::Shared,
	},
	anyhow::Context,
	egui::Widget,
	std::{path::Path, sync::Arc},
	tokio::sync::RwLock,
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
	pub fn draw(&mut self, ctx: &egui::Context, shared: &Arc<Shared>) {
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
				Tab::Panels => self::draw_panels_tab(&mut self.add_playlist_state, ui, shared),
				Tab::Playlists => self::draw_playlists(&mut self.add_playlist_state, ui, shared),
			}
		});
	}
}
/// Draws the panels tab
fn draw_panels_tab(add_playlist_state: &mut AddPlaylistState, ui: &mut egui::Ui, shared: &Arc<Shared>) {
	self::draw_panels_editor(add_playlist_state, ui, shared);
	ui.separator();
	self::draw_shader_select(ui, shared);
}

/// Draws the playlists tab
fn draw_playlists(add_playlist_state: &mut AddPlaylistState, ui: &mut egui::Ui, shared: &Arc<Shared>) {
	let playlists = shared.playlists.blocking_read().get_all();

	for (playlist_name, playlist) in playlists {
		let playlist_path = shared.playlists.blocking_read().playlist_path(&playlist_name);
		ui.collapsing(format!("{playlist_name} ({playlist_path:?})"), |ui| {
			let items = playlist.read().block_on().items();

			for item in items {
				let mut item = item.write().block_on();

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
					let playlist_name = playlist_name.clone();
					let shared = Arc::clone(shared);
					crate::spawn_task(format!("Reload playlist {playlist_name:?}"), || async move {
						shared
							.playlists
							.write()
							.await
							.reload(playlist_name)
							.await
							.context("Unable to reload playlist")?;

						Ok(())
					});
				}

				if ui.button("💾 (Save)").clicked() {
					let playlist_name = playlist_name.clone();
					let shared = Arc::clone(shared);
					crate::spawn_task(format!("Saving playlist {playlist_name:?}"), || async move {
						shared
							.playlists
							.read()
							.await
							.save(&playlist_name)
							.await
							.context("Unable to save playlist")?;

						Ok(())
					});
				}

				ui.separator();
			}
		});
	}

	if ui.button("➕ (Add playlist)").clicked() {
		// DEADLOCK: We have the locker setup such that advancing from 0 to 2 cannot deadlock
		self::choose_load_playlist_from_file(add_playlist_state, shared);
	}
}

/// Asks the user and loads a playlist from a file
fn choose_load_playlist_from_file(
	_add_playlist_state: &mut AddPlaylistState,
	shared: &Arc<Shared>,
) -> Option<(PlaylistName, Arc<RwLock<Playlist>>)> {
	// TODO: Not have this yaml filter here? Or at least allow files other than `.yaml`
	let file_dialog = rfd::FileDialog::new().add_filter("Playlist file", &["yaml"]);

	// Ask the user for a playlist file
	match file_dialog.pick_file() {
		// If we got it, try to load it
		Some(playlist_path) => {
			tracing::debug!(?playlist_path, "Loading playlist");

			// DEADLOCK: We have the locker setup such that advancing from 0 to 2 cannot deadlock
			let res = shared.playlists.blocking_write().add(&playlist_path).block_on();
			match res {
				Ok((playlist_name, playlist)) => {
					tracing::debug!(?playlist_name, ?playlist, "Successfully loaded playlist");
					return Some((playlist_name, playlist));
				},
				Err(err) => tracing::warn!(?playlist_path, ?err, "Unable to load playlist"),
			}
		},

		// Else just log that the user cancelled it
		None => tracing::debug!("User cancelled load playlist"),
	}

	None
}

/// Draws the panels editor
// TODO: Not edit the values as-is, as that breaks some invariants of panels (such as duration versus image states)
fn draw_panels_editor(add_playlist_state: &mut AddPlaylistState, ui: &mut egui::Ui, shared: &Arc<Shared>) {
	let mut cur_panels = shared.cur_panels.lock().block_on();

	if cur_panels.is_empty() {
		ui.label("None loaded");
		return;
	}

	for (panel_idx, panel) in cur_panels.iter_mut().enumerate() {
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
				egui::Slider::new(&mut panel.state.progress, 0..=panel.state.duration.saturating_sub(1))
					.clamp_to_range(true)
					.ui(ui);

				// Then clamp to the current max
				// Note: We don't just use this max above so the slider doesn't jitter when the max changes
				let cur_max = match (panel.images.cur.is_loaded(), panel.images.next.is_loaded()) {
					(false, false) => 0,
					(true, false) => panel.state.fade_point,
					(_, true) => panel.state.duration,
				};
				panel.state.progress = panel.state.progress.clamp(0, cur_max);
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
					match (panel.images.cur.is_loaded(), panel.images.next.is_loaded()) {
						(false, false) => (),
						(true, false) => panel.state.progress = panel.state.fade_point,
						(_, true) => panel.state.progress = panel.state.duration,
					}
				}
			});

			ui.collapsing("Images", |ui| {
				ui.collapsing("Previous", |ui| match panel.images.prev.is_loaded() {
					true => self::draw_panel_image(ui, &mut panel.images.prev),
					false => {
						ui.label("[Unloaded]");
					},
				});
				ui.collapsing("Current", |ui| match panel.images.cur.is_loaded() {
					true => self::draw_panel_image(ui, &mut panel.images.cur),
					false => {
						ui.label("[Unloaded]");
					},
				});
				ui.collapsing("Next", |ui| match panel.images.next.is_loaded() {
					true => self::draw_panel_image(ui, &mut panel.images.next),
					false => {
						ui.label("[Unloaded]");
					},
				});
			});

			ui.collapsing("Playlist player", |ui| {
				let playlist_player = panel.playlist_player.write().block_on();

				let row_height = ui.text_style_height(&egui::TextStyle::Body);

				if ui.button("↹ (Replace)").clicked() {
					// TODO: Stop everything that could be inserting items still?
					if let Some((playlist_name, playlist)) =
						self::choose_load_playlist_from_file(add_playlist_state, shared)
					{
						crate::spawn_task(format!("Replace playlist {playlist:?}"), {
							let playlist_player = Arc::clone(&panel.playlist_player);
							let shared = Arc::clone(shared);
							|| async move {
								{
									let mut playlist_player = playlist_player.write().await;
									playlist_player.remove_all();
								}

								PanelsManager::load_playlist_into(&playlist_player, &playlist_name, &shared)
									.await
									.context("Unable to load playlist")?;

								Ok(())
							}
						});
					}
				};

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
fn draw_shader_select(ui: &mut egui::Ui, shared: &Shared) {
	ui.label("Shader");

	let mut panels_renderer_shader = shared.panels_renderer_shader.write().block_on();
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
struct AddPlaylistState {}
