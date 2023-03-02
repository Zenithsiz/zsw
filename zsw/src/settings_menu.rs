//! Settings menu

// Lints
#![allow(unused_results)] // Egui produces a lot of results we don't need to use
#![expect(clippy::too_many_lines)] // TODO: Refactor

// Imports
use {
	crate::{
		panel::{self, PanelGroup, PanelImage, PanelShader, PanelsRendererShader},
		playlist::{PlaylistItem, PlaylistsManager},
		shared::{Locker, LockerExt, Shared},
	},
	egui::Widget,
	std::path::Path,
	zsw_util::Rect,
};

/// Settings menu
#[derive(Debug)]
pub struct SettingsMenu {
	/// If open
	open: bool,

	/// Current tab
	cur_tab: Tab,
}

impl SettingsMenu {
	/// Creates the settings menu
	pub fn new() -> Self {
		Self {
			open:    false,
			cur_tab: Tab::Panels,
		}
	}

	/// Draws the settings menu
	pub fn draw(&mut self, ctx: &egui::Context, _frame: &epi::Frame, shared: &Shared, locker: &mut Locker) {
		// Adjust cursor pos to account for the scale factor
		let scale_factor = shared.window.scale_factor();
		let cursor_pos = shared.cursor_pos.load().to_logical::<f32>(scale_factor);

		// Create the window
		let mut egui_window = egui::Window::new("Settings");

		// Open it at the mouse if pressed
		if ctx.input().pointer.button_clicked(egui::PointerButton::Secondary) {
			egui_window = egui_window.current_pos(egui::pos2(cursor_pos.x, cursor_pos.y));
			self.open = true;
		}

		// Then render it
		egui_window.open(&mut self.open).show(ctx, |ui| {
			//egui::TopBottomPanel::top("Settings - Toolbar").show(ctx, add_contents)

			ui.horizontal(|ui| {
				ui.selectable_value(&mut self.cur_tab, Tab::Panels, "Panels");
				ui.selectable_value(&mut self.cur_tab, Tab::Playlists, "Playlists");
			});
			ui.separator();

			match self.cur_tab {
				Tab::Panels => self::draw_panels_tab(ui, shared, locker),
				Tab::Playlists => self::draw_playlists(ui, shared, locker),
			}
		});
	}
}
/// Draws the panels tab
fn draw_panels_tab(ui: &mut egui::Ui, shared: &Shared, locker: &mut Locker) {
	self::draw_panels_editor(ui, shared, locker);
	ui.separator();
	self::draw_shader_select(ui, shared, locker);
}

/// Draws the playlists tab
fn draw_playlists(ui: &mut egui::Ui, shared: &Shared, locker: &mut Locker) {
	let (mut playlists_manager, _) = locker.blocking_rwlock_write::<PlaylistsManager>(&shared.playlists_manager);
	for (name, playlist) in playlists_manager.get_all_mut() {
		ui.collapsing(name, |ui| {
			for item in playlist.items_mut() {
				match item {
					PlaylistItem::Directory { path, recursive } => {
						ui.horizontal(|ui| {
							ui.label("Dir: ");
							self::draw_openable_path(ui, path);
						});

						ui.checkbox(recursive, "Recursive");
					},
					PlaylistItem::File { path } => {
						ui.horizontal(|ui| {
							ui.label("File: ");
							self::draw_openable_path(ui, path);
						});
					},
				}
			}
		});
	}
}


/// Draws the panels editor
// TODO: Not edit the values as-is, as that breaks some invariants of panels (such as duration versus image states)
fn draw_panels_editor(ui: &mut egui::Ui, shared: &Shared, locker: &mut Locker) {
	let (mut panel_group, _) = locker.blocking_mutex_lock::<Option<PanelGroup>>(&shared.cur_panel_group);
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

					ui.collapsing("Playlist player", |ui| {
						let row_height = ui.text_style_height(&egui::TextStyle::Body);

						ui.collapsing("Next", |ui| {
							egui::ScrollArea::new([false, true])
								.auto_shrink([false, true])
								.stick_to_right(true)
								.max_height(row_height * 10.0)
								.show_rows(
									ui,
									row_height,
									panel.playlist_player.peek_next_items().len(),
									|ui, idx| {
										for item in
											panel.playlist_player.peek_next_items().take(idx.end).skip(idx.start)
										{
											self::draw_openable_path(ui, item);
										}
									},
								);
						});

						ui.collapsing("All", |ui| {
							egui::ScrollArea::new([false, true])
								.auto_shrink([false, true])
								.stick_to_right(true)
								.max_height(row_height * 10.0)
								.show_rows(ui, row_height, panel.playlist_player.all_items().len(), |ui, idx| {
									for item in panel.playlist_player.all_items().take(idx.end).skip(idx.start) {
										self::draw_openable_path(ui, item);
									}
								});
						});
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
fn draw_shader_select(ui: &mut egui::Ui, shared: &Shared, locker: &mut Locker) {
	ui.label("Shader");

	let (mut panels_renderer_shader, _) =
		locker.blocking_rwlock_write::<PanelsRendererShader>(&shared.panels_renderer_shader);
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
