//! Panels tab

// Imports
use {
	crate::{
		display::Display,
		panel::{PanelFadeImage, PanelFadeShader, PanelFadeState, PanelNoneState, PanelState, Panels},
	},
	core::time::Duration,
	egui::Widget,
	zsw_util::{Rect, TokioTaskBlockOn},
	zsw_wgpu::Wgpu,
};

/// Draws the panels tab
pub fn draw_panels_tab(ui: &mut egui::Ui, wgpu: &Wgpu, panels: &Panels, window_geometry: Rect<i32, u32>) {
	self::draw_panels_editor(ui, wgpu, panels, window_geometry);
	ui.separator();
}

/// Draws the panels editor
// TODO: Not edit the values as-is, as that breaks some invariants of panels (such as duration versus image states)
fn draw_panels_editor(ui: &mut egui::Ui, wgpu: &Wgpu, panels: &Panels, window_geometry: Rect<i32, u32>) {
	let mut panels = panels.get_all().block_on();
	if panels.is_empty() {
		ui.label("None loaded");
		return;
	}

	for panel in &mut *panels {
		let mut display = panel.display.lock().block_on();

		let mut name = egui::WidgetText::from(display.name.to_string());
		if display
			.geometries
			.iter()
			.all(|&geometry| window_geometry.intersection(geometry).is_none())
		{
			name = name.weak();
		}

		ui.collapsing(name, |ui| {
			match &mut panel.state {
				PanelState::None(_) => (),
				PanelState::Fade(state) => self::draw_fade_panel_editor(ui, wgpu, window_geometry, state, &mut display),
			}

			ui.collapsing("Shader", |ui| {
				self::draw_shader_select(ui, &mut panel.state);
			});
		});
	}
}

/// Draws the fade panel editor
fn draw_fade_panel_editor(
	ui: &mut egui::Ui,
	wgpu: &Wgpu,
	window_geometry: Rect<i32, u32>,
	panel_state: &mut PanelFadeState,
	display: &mut Display,
) {
	{
		let mut is_paused = panel_state.is_paused();
		ui.checkbox(&mut is_paused, "Paused");
		panel_state.set_paused(is_paused);
	}

	ui.collapsing("Geometries", |ui| {
		for (geometry_idx, geometry) in display.geometries.iter_mut().enumerate() {
			ui.horizontal(|ui| {
				let mut name = egui::WidgetText::from(format!("#{}: ", geometry_idx + 1));
				if window_geometry.intersection(*geometry).is_none() {
					name = name.weak();
				}

				ui.label(name);
				super::draw_rect(ui, geometry);
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
		super::draw_duration(ui, &mut progress, Duration::ZERO..=max);
		panel_state.set_progress(progress);
	});

	ui.horizontal(|ui| {
		ui.label("Fade Duration");
		let min = Duration::ZERO;
		let max = panel_state.duration() / 2;

		let mut fade_duration = panel_state.fade_duration();
		super::draw_duration(ui, &mut fade_duration, min..=max);
		panel_state.set_fade_duration(fade_duration);
	});

	ui.horizontal(|ui| {
		ui.label("Duration");

		let mut duration = panel_state.duration();
		super::draw_duration(ui, &mut duration, Duration::ZERO..=Duration::from_secs_f32(180.0));
		panel_state.set_duration(duration);
	});

	ui.horizontal(|ui| {
		ui.label("Skip");
		if ui.button("🔄").clicked() {
			panel_state.skip(wgpu).block_on();
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
		let playlist_player = panel_state.playlist_player().lock().block_on();

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
						super::draw_openable_path(ui, item);
					}
				});
		});
	});
}

/// Draws a panel image
fn draw_panel_image(ui: &mut egui::Ui, image: &mut Option<PanelFadeImage>) {
	match image {
		None => {
			ui.label("[Unloaded]");
		},
		Some(image) => {
			super::draw_openable_path(ui, &image.path);

			let texture = image.texture_view.texture();
			ui.label(format!("Size: {}x{}", texture.width(), texture.height()));
			ui.checkbox(&mut image.swap_dir, "Swap direction");
		},
	}
}

/// Draws the shader select
fn draw_shader_select(ui: &mut egui::Ui, state: &mut PanelState) {
	egui::ComboBox::from_id_salt("Shader selection menu")
		.selected_text(match state {
			PanelState::None(_) => "None",
			PanelState::Fade(_) => "Fade",
		})
		.show_ui(ui, |ui| {
			// TODO: Review these defaults?
			type CreateShader = fn() -> PanelState;
			let create_shaders: [(_, _, CreateShader); _] = [
				("None", matches!(state, PanelState::None(_)), || {
					PanelState::None(PanelNoneState::new([0.0; 4]))
				}),
				("Fade", matches!(state, PanelState::Fade(_)), || {
					PanelState::Fade(PanelFadeState::new(
						Duration::from_secs(60),
						Duration::from_secs(5),
						PanelFadeShader::Out { strength: 1.5 },
					))
				}),
			];

			for (name, checked, create_shader) in create_shaders {
				if ui.selectable_label(checked, name).clicked() && !checked {
					*state = create_shader();
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
						PanelFadeShader::Basic,
						PanelFadeShader::White { strength: 1.0 },
						PanelFadeShader::Out { strength: 0.2 },
						PanelFadeShader::In { strength: 0.2 },
					];
					for shader in shaders {
						ui.selectable_value(state.shader_mut(), shader, shader.name());
					}
				});

			match state.shader_mut() {
				PanelFadeShader::Basic => (),
				PanelFadeShader::White { strength } => {
					ui.horizontal(|ui| {
						ui.label("Strength");
						egui::Slider::new(strength, 0.0..=20.0).ui(ui);
					});
				},
				PanelFadeShader::Out { strength } => {
					ui.horizontal(|ui| {
						ui.label("Strength");
						egui::Slider::new(strength, 0.0..=2.0).ui(ui);
					});
				},
				PanelFadeShader::In { strength } => {
					ui.horizontal(|ui| {
						ui.label("Strength");
						egui::Slider::new(strength, 0.0..=2.0).ui(ui);
					});
				},
			}
		},
	}
}
