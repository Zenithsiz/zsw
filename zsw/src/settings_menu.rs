//! Settings menu

// Lints
#![allow(unused_results)] // Egui produces a lot of results we don't need to use

// Imports
use {
	crate::{
		panel::{PanelFadeState, PanelGeometry, PanelImage, PanelNoneState, PanelShaderFade, PanelState},
		playlist::PlaylistName,
		shared::{Shared, SharedWindow},
	},
	core::{ops::RangeInclusive, time::Duration},
	egui::Widget,
	std::{path::Path, sync::Arc},
	zsw_util::{AppError, Rect, TokioTaskBlockOn},
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
	pub fn draw(&mut self, ctx: &egui::Context, shared: &Shared, shared_window: &SharedWindow) {
		// Adjust cursor pos to account for the scale factor
		let scale_factor = shared_window.window.scale_factor();
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
				ui.selectable_value(&mut self.cur_tab, Tab::Settings, "Settings");
			});
			ui.separator();

			match self.cur_tab {
				Tab::Panels => self::draw_panels_tab(ui, shared, shared_window),
				Tab::Settings => self::draw_settings(ui, shared),
			}
		});
	}
}
/// Draws the panels tab
fn draw_panels_tab(ui: &mut egui::Ui, shared: &Shared, shared_window: &SharedWindow) {
	self::draw_panels_editor(ui, shared, shared_window);
	ui.separator();
}

/// Draws the panels editor
// TODO: Not edit the values as-is, as that breaks some invariants of panels (such as duration versus image states)
fn draw_panels_editor(ui: &mut egui::Ui, shared: &Shared, shared_window: &SharedWindow) {
	let panels = shared.panels.get_all().block_on();

	if panels.is_empty() {
		ui.label("None loaded");
		return;
	}

	for panel in panels {
		let mut panel = panel.lock().block_on();
		let panel = &mut *panel;

		let mut name = egui::WidgetText::from(panel.name.to_string());
		if panel
			.geometries
			.iter()
			.all(|geometry| shared_window.monitor_geometry.intersection(geometry.geometry).is_none())
		{
			name = name.weak();
		}

		ui.collapsing(name, |ui| {
			match &mut panel.state {
				PanelState::None(_) => (),
				PanelState::Fade(state) =>
					self::draw_fade_panel_editor(ui, shared, shared_window, state, &mut panel.geometries),
			}

			ui.collapsing("Shader", |ui| {
				self::draw_shader_select(ui, shared, &mut panel.state);
			});
		});
	}
}

/// Draws the fade panel editor
fn draw_fade_panel_editor(
	ui: &mut egui::Ui,
	shared: &Shared,
	shared_window: &SharedWindow,
	panel_state: &mut PanelFadeState,
	geometries: &mut [PanelGeometry],
) {
	{
		let mut is_paused = panel_state.is_paused();
		ui.checkbox(&mut is_paused, "Paused");
		panel_state.set_paused(is_paused);
	}

	ui.collapsing("Geometries", |ui| {
		for (geometry_idx, geometry) in geometries.iter_mut().enumerate() {
			ui.horizontal(|ui| {
				let mut name = egui::WidgetText::from(format!("#{}: ", geometry_idx + 1));
				if shared_window.monitor_geometry.intersection(geometry.geometry).is_none() {
					name = name.weak();
				}

				ui.label(name);
				self::draw_rect(ui, &mut geometry.geometry);
			});
		}
	});

	ui.horizontal(|ui| {
		ui.label("Cur progress");

		// Note: We only allow up until the duration - 1 so that you don't get stuck
		//       skipping images when you hold it at the max value
		// TODO: This max needs to be `duration - min_frame_duration` to not skip ahead.
		let max = panel_state.duration().mul_f32(0.99);
		let mut progress = panel_state.progress();
		self::draw_duration(ui, &mut progress, Duration::ZERO..=max);
		panel_state.set_progress(progress);
	});

	ui.horizontal(|ui| {
		ui.label("Fade Duration");
		let min = Duration::ZERO;
		let max = panel_state.duration() / 2;

		let mut fade_duration = panel_state.fade_duration();
		self::draw_duration(ui, &mut fade_duration, min..=max);
		panel_state.set_fade_duration(fade_duration);
	});

	ui.horizontal(|ui| {
		ui.label("Duration");

		let mut duration = panel_state.duration();
		self::draw_duration(ui, &mut duration, Duration::ZERO..=Duration::from_secs_f32(180.0));
		panel_state.set_duration(duration);
	});

	ui.horizontal(|ui| {
		ui.label("Skip");
		if ui.button("🔄").clicked() {
			panel_state.skip(shared.wgpu);
		}
	});

	ui.collapsing("Images", |ui| {
		ui.collapsing("Previous", |ui| {
			self::draw_panel_image(ui, &mut panel_state.images_mut().prev);
		});
		ui.collapsing("Current", |ui| {
			self::draw_panel_image(ui, &mut panel_state.images_mut().cur);
		});
		ui.collapsing("Next", |ui| {
			self::draw_panel_image(ui, &mut panel_state.images_mut().next);
		});
	});

	ui.collapsing("Playlist", |ui| {
		ui.label(format!("Playlist: {:?}", panel_state.playlist()));

		let Some(playlist_player) = panel_state.playlist_player() else {
			ui.weak("Not loaded");
			return;
		};

		let row_height = ui.text_style_height(&egui::TextStyle::Body);

		ui.label(format!("Position: {}", playlist_player.cur_pos()));

		ui.label(format!(
			"Remaining until shuffle: {}",
			playlist_player.remaining_until_shuffle()
		));

		ui.collapsing("Items", |ui| {
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
	});
}

/// Draws the settings tab
fn draw_settings(ui: &mut egui::Ui, shared: &Shared) {
	if ui.button("Quit").clicked() {
		shared
			.event_loop_proxy
			.send_event(crate::AppEvent::Shutdown)
			.expect("Unable to send shutdown event to event loop");
	}
}

/// Draws an openable path
fn draw_openable_path(ui: &mut egui::Ui, path: &Path) {
	ui.horizontal(|ui| {
		ui.label("Path: ");
		// TODO: Not use lossy conversion to display it?
		if ui.link(path.to_string_lossy()).clicked() &&
			let Err(err) = opener::open(path)
		{
			let err = AppError::new(&err);
			tracing::warn!("Unable to open file {path:?}: {}", err.pretty());
		}
	});
}

/// Draws a panel image
fn draw_panel_image(ui: &mut egui::Ui, image: &mut PanelImage) {
	match image {
		PanelImage::Empty => {
			ui.label("[Unloaded]");
		},
		PanelImage::Loaded {
			size,
			swap_dir,
			image_path,
			..
		} => {
			self::draw_openable_path(ui, image_path);
			ui.label(format!("Size: {}x{}", size.x, size.y));
			ui.checkbox(swap_dir, "Swap direction");
		},
	}
}

/// Draws the shader select
fn draw_shader_select(ui: &mut egui::Ui, shared: &Shared, state: &mut PanelState) {
	egui::ComboBox::from_id_salt("Shader selection menu")
		.selected_text(match state {
			PanelState::None(_) => "None",
			PanelState::Fade(_) => "Fade",
		})
		.show_ui(ui, |ui| {
			// TODO: Review these defaults?
			type CreateShader = fn(&Shared) -> PanelState;
			let create_shaders: [(_, _, CreateShader); _] = [
				("None", matches!(state, PanelState::None(_)), |_| {
					PanelState::None(PanelNoneState::new([0.0; 4]))
				}),
				("Fade", matches!(state, PanelState::Fade(_)), |shared| {
					// TODO: We don't have a playlist to pass here, so should we make
					//       the playlist optional and have the user later change it?
					PanelState::Fade(PanelFadeState::new(
						Duration::from_secs(60),
						Duration::from_secs(5),
						PanelShaderFade::Out { strength: 1.5 },
						PlaylistName::from("none".to_owned()),
						Arc::clone(&shared.playlists),
					))
				}),
			];

			for (name, checked, create_shader) in create_shaders {
				if ui.selectable_label(checked, name).clicked() && !checked {
					*state = create_shader(shared);
				}
			}
		});

	match state {
		PanelState::None(state) =>
			_ = ui.horizontal(|ui| {
				ui.label("Background color");
				let mut color = egui::Rgba::from_rgba_premultiplied(
					state.background_color[0],
					state.background_color[1],
					state.background_color[2],
					state.background_color[3],
				);
				egui::color_picker::color_edit_button_rgba(ui, &mut color, egui::color_picker::Alpha::OnlyBlend);
				state.background_color = color.to_array();
			}),
		PanelState::Fade(state) => {
			egui::ComboBox::from_id_salt("Fade shader menu")
				.selected_text(state.shader().name())
				.show_ui(ui, |ui| {
					// TODO: Not have default values here?
					let shaders = [
						PanelShaderFade::Basic,
						PanelShaderFade::White { strength: 1.0 },
						PanelShaderFade::Out { strength: 0.2 },
						PanelShaderFade::In { strength: 0.2 },
					];
					for shader in shaders {
						ui.selectable_value(state.shader_mut(), shader, shader.name());
					}
				});

			match state.shader_mut() {
				PanelShaderFade::Basic => (),
				PanelShaderFade::White { strength } => {
					ui.horizontal(|ui| {
						ui.label("Strength");
						egui::Slider::new(strength, 0.0..=20.0).ui(ui);
					});
				},
				PanelShaderFade::Out { strength } => {
					ui.horizontal(|ui| {
						ui.label("Strength");
						egui::Slider::new(strength, 0.0..=2.0).ui(ui);
					});
				},
				PanelShaderFade::In { strength } => {
					ui.horizontal(|ui| {
						ui.label("Strength");
						egui::Slider::new(strength, 0.0..=2.0).ui(ui);
					});
				},
			}
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

/// Draws a duration slider
// TODO: Allow setting the clamping mode by using a builder instead
fn draw_duration(ui: &mut egui::Ui, duration: &mut Duration, range: RangeInclusive<Duration>) {
	let mut secs = duration.as_secs_f32();

	let start = range.start().as_secs_f32();
	let end = range.end().as_secs_f32();
	egui::Slider::new(&mut secs, start..=end)
		.suffix("s")
		.clamping(egui::SliderClamping::Edits)
		.ui(ui);
	*duration = Duration::from_secs_f32(secs);
}

/// Tab
#[derive(PartialEq, Debug)]
enum Tab {
	Panels,
	Settings,
}
