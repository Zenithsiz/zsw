//! Settings menu

// Lints
#![allow(unused_results)] // Egui produces a lot of results we don't need to use

// Imports
use {
	crate::{
		panel::{PanelImage, PanelShader},
		shared::{Shared, SharedWindow},
	},
	core::sync::atomic,
	egui::Widget,
	std::path::Path,
	zsw_util::{Rect, TokioTaskBlockOn},
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
	let mut cur_panels = shared.cur_panels.lock().block_on();

	if cur_panels.is_empty() {
		ui.label("None loaded");
		return;
	}

	for panel in cur_panels.iter_mut() {
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
				egui::Slider::new(&mut panel.state.progress, 0..=panel.state.duration.saturating_sub(1))
					.clamping(egui::SliderClamping::Always)
					.ui(ui);

				// Then clamp to the current max
				// Note: We don't just use this max above so the slider doesn't jitter when the max changes
				let cur_max = match (panel.images.cur().is_loaded(), panel.images.next().is_loaded()) {
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
				egui::Slider::new(&mut panel.state.duration, 0..=10800)
					.clamping(egui::SliderClamping::Never)
					.ui(ui);
			});

			ui.horizontal(|ui| {
				ui.label("Skip");
				if ui.button("🔄").clicked() {
					panel
						.skip(shared.wgpu, &shared.panels_renderer_layouts, &shared.image_requester)
						.block_on();
				}
			});

			ui.collapsing("Images", |ui| {
				ui.collapsing("Previous", |ui| self::draw_panel_image(ui, panel.images.prev_mut()));
				ui.collapsing("Current", |ui| self::draw_panel_image(ui, panel.images.cur_mut()));
				ui.collapsing("Next", |ui| self::draw_panel_image(ui, panel.images.next_mut()));
			});

			ui.collapsing("Playlist player", |ui| {
				let playlist_player = panel.images.playlist_player().lock().block_on();

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

			ui.collapsing("Shader", |ui| self::draw_shader_select(ui, &mut panel.shader));
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

	let mut panels_update_render_paused = shared.panels_update_render_paused.load(atomic::Ordering::Acquire);
	if ui
		.checkbox(&mut panels_update_render_paused, "Panels update/render paused")
		.changed()
	{
		shared
			.panels_update_render_paused
			.store(panels_update_render_paused, atomic::Ordering::Release);
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
			tracing::warn!(?path, %err, "Unable to open file");
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


/// Tab
#[derive(PartialEq, Debug)]
enum Tab {
	Panels,
	Settings,
}
