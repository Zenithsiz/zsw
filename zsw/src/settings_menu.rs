//! Settings menu

// Lints
#![allow(unused_results)] // Egui produces a lot of results we don't need to use

// Imports
use {
	crate::{
		panel::{PanelImage, PanelShader},
		shared::{Shared, SharedWindow},
	},
	core::{ops::RangeInclusive, time::Duration},
	egui::Widget,
	std::path::Path,
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
#[expect(clippy::too_many_lines, reason = "TODO: Split it up")]
fn draw_panels_editor(ui: &mut egui::Ui, shared: &Shared, shared_window: &SharedWindow) {
	let panels = shared.panels.get_all().block_on();
	let mut panels_images = shared.panels_images.lock().block_on();

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
			ui.checkbox(&mut panel.state.paused, "Paused");

			ui.collapsing("Geometries", |ui| {
				for (geometry_idx, geometry) in panel.geometries.iter_mut().enumerate() {
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
				// Note: We only allow up until the duration - 1 so that you don't get stuck
				//       skipping images when you hold it at the max value
				ui.label("Cur progress");
				self::draw_duration(
					ui,
					&mut panel.state.progress,
					// TODO: This max needs to be `duration - min_frame_duration` to not skip ahead.
					Duration::ZERO..=panel.state.duration.mul_f32(0.99),
				);


				// Then clamp to the current max
				// Note: We don't just use this max above so the slider doesn't jitter when the max changes
				let cur_max = match panels_images.get(&panel.name) {
					Some(panel_images) => match (panel_images.cur.is_loaded(), panel_images.next.is_loaded()) {
						(false, false) => Duration::ZERO,
						(true, false) => panel.state.fade_duration,
						(_, true) => panel.state.duration,
					},
					None => Duration::ZERO,
				};

				// TODO: This should be done elsewhere.
				panel.state.progress = panel.state.progress.clamp(Duration::ZERO, cur_max);
			});

			ui.horizontal(|ui| {
				ui.label("Fade Duration");
				let min = Duration::ZERO;
				let max = panel.state.duration / 2;

				self::draw_duration(ui, &mut panel.state.fade_duration, min..=max);
			});

			ui.horizontal(|ui| {
				ui.label("Duration");
				self::draw_duration(
					ui,
					&mut panel.state.duration,
					Duration::ZERO..=Duration::from_secs_f32(180.0),
				);
			});

			ui.horizontal(|ui| {
				let Some(panel_images) = panels_images.get_mut(&panel.name) else {
					return;
				};

				ui.label("Skip");
				if ui.button("🔄").clicked() {
					panel.skip(panel_images, shared.wgpu, &shared.panels_renderer_layouts);
				}
			});

			ui.collapsing("Images", |ui| {
				let Some(panel_images) = panels_images.get_mut(&panel.name) else {
					ui.weak("Not loaded");
					return;
				};
				ui.collapsing("Previous", |ui| self::draw_panel_image(ui, &mut panel_images.prev));
				ui.collapsing("Current", |ui| self::draw_panel_image(ui, &mut panel_images.cur));
				ui.collapsing("Next", |ui| self::draw_panel_image(ui, &mut panel_images.next));
			});

			ui.collapsing("Playlist player", |ui| {
				let Some(panel_images) = panels_images.get_mut(&panel.name) else {
					ui.weak("Not loaded");
					return;
				};

				let row_height = ui.text_style_height(&egui::TextStyle::Body);

				ui.label(format!("Position: {}", panel_images.playlist_player.cur_pos()));

				ui.label(format!(
					"Remaining until shuffle: {}",
					panel_images.playlist_player.remaining_until_shuffle()
				));

				ui.collapsing("Items", |ui| {
					egui::ScrollArea::new([false, true])
						.auto_shrink([false, true])
						.stick_to_right(true)
						.max_height(row_height * 10.0)
						.show_rows(
							ui,
							row_height,
							panel_images.playlist_player.all_items().len(),
							|ui, idx| {
								for item in panel_images.playlist_player.all_items().take(idx.end).skip(idx.start) {
									self::draw_openable_path(ui, item);
								}
							},
						);
				});
			});

			ui.collapsing("Shader", |ui| self::draw_shader_select(ui, &mut panel.state.shader));
		});
	}
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
fn draw_shader_select(ui: &mut egui::Ui, cur_shader: &mut PanelShader) {
	egui::ComboBox::from_id_salt("Shader selection menu")
		.selected_text(cur_shader.name())
		.show_ui(ui, |ui| {
			// TODO: Not have default values here?
			let shaders = [
				PanelShader::None {
					background_color: [0.0; 4],
				},
				PanelShader::Fade,
				PanelShader::FadeWhite { strength: 1.0 },
				PanelShader::FadeOut { strength: 0.2 },
				PanelShader::FadeIn { strength: 0.2 },
			];
			for shader in shaders {
				ui.selectable_value(cur_shader, shader, shader.name());
			}
		});

	match &mut *cur_shader {
		PanelShader::None {
			background_color: bg_color,
		} => {
			ui.horizontal(|ui| {
				ui.label("Background color");
				let mut color = egui::Rgba::from_rgba_premultiplied(bg_color[0], bg_color[1], bg_color[2], bg_color[3]);
				egui::color_picker::color_edit_button_rgba(ui, &mut color, egui::color_picker::Alpha::OnlyBlend);
				*bg_color = color.to_array();
			});
		},
		PanelShader::Fade => (),
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
