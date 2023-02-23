//! Settings menu

// Lints
#![allow(unused_results)] // Egui produces a lot of results we don't need to use

// Imports
use {
	crate::panel::{self, PanelGroup, PanelImage, PanelShader, PanelsRendererShader},
	egui::Widget,
	winit::dpi::PhysicalPosition,
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
	pub fn draw(
		&mut self,
		ctx: &egui::Context,
		_frame: &epi::Frame,
		window: &winit::window::Window,
		cursor_pos: PhysicalPosition<f64>,
		panel_group: &mut Option<PanelGroup>,
		panels_renderer_shader: &mut PanelsRendererShader,
	) {
		// Adjust cursor pos to account for the scale factor
		let scale_factor = window.scale_factor();
		let cursor_pos = cursor_pos.to_logical::<f32>(scale_factor);

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
			});
			ui.separator();

			match self.cur_tab {
				Tab::Panels => self::draw_panels_tab(ui, panel_group, panels_renderer_shader),
			}
		});
	}
}

/// Draws the panels tab
fn draw_panels_tab(
	ui: &mut egui::Ui,
	panel_group: &mut Option<PanelGroup>,
	panels_renderer_shader: &mut PanelsRendererShader,
) {
	self::draw_panels_editor(ui, panel_group);
	ui.separator();
	self::draw_shader_select(ui, panels_renderer_shader);
}

/// Draws the panels editor
// TODO: Not edit the values as-is, as that breaks some invariants of panels (such as duration versus image states)
fn draw_panels_editor(ui: &mut egui::Ui, panel_group: &mut Option<PanelGroup>) {
	match panel_group {
		Some(panel_group) =>
			for (panel_idx, panel) in panel_group.panels_mut().iter_mut().enumerate() {
				ui.collapsing(format!("Panel {panel_idx}"), |ui| {
					ui.collapsing("Geometries", |ui| {
						for (geometry_idx, geometry) in panel.geometries.iter_mut().enumerate() {
							ui.horizontal(|ui| {
								ui.label(format!("#{}: ", geometry_idx + 1));
								self::draw_rect(ui, &mut geometry.geometry);
							});
						}
					});


					ui.horizontal(|ui| {
						ui.label("Cur progress");
						let max = panel.state.duration.saturating_sub(1);
						egui::Slider::new(&mut panel.state.cur_progress, 0..=max).ui(ui);
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
				});
			},
		None => {
			ui.label("None loaded");
		},
	}
}

/// Draws a panel image
fn draw_panel_image(ui: &mut egui::Ui, image: &mut PanelImage) {
	let size = image.size();
	if let Some(name) = image.name() {
		ui.label(format!("Name: {name}"));
	}
	ui.label(format!("Size: {}x{}", size.x, size.y));
	ui.checkbox(image.swap_dir_mut(), "Swap direction");
}

/// Draws the shader select
fn draw_shader_select(ui: &mut egui::Ui, panels_renderer_shader: &mut PanelsRendererShader) {
	ui.label("Shader");

	let cur_shader = &mut panels_renderer_shader.shader;
	egui::ComboBox::from_id_source("Shader selection menu")
		.selected_text(cur_shader.name())
		.show_ui(ui, |ui| {
			// TODO: Not have default values here?
			let shaders = [
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
}
