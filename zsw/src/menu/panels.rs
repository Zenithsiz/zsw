//! Panels tab

// Imports
use {
	crate::panel::{
		PanelGeometry,
		PanelState,
		Panels,
		state::{PanelFadeState, fade::PanelFadeImage},
	},
	core::time::Duration,
	std::ptr,
	zsw_util::Rect,
	zsw_wgpu::Wgpu,
};

/// Draws the panels tab
pub fn draw_panels_tab(ui: &mut egui::Ui, wgpu: &Wgpu, panels: &mut Panels, window_geometry: Rect<i32, u32>) {
	self::draw_panels_editor(ui, wgpu, panels, window_geometry);
	ui.separator();
}

/// Draws the panels editor
// TODO: Not edit the values as-is, as that breaks some invariants of panels (such as duration versus image states)
fn draw_panels_editor(ui: &mut egui::Ui, wgpu: &Wgpu, panels: &mut Panels, window_geometry: Rect<i32, u32>) {
	let panels = panels.get_all();
	if panels.is_empty() {
		ui.label("None loaded");
		return;
	}

	for (panel_idx, panel) in panels.iter_mut().enumerate() {
		let mut name = egui::WidgetText::from(format!("Panel #{panel_idx}"));
		if panel
			.geometries
			.iter()
			.all(|geometry| !geometry.intersects_window(window_geometry))
		{
			name = name.weak();
		}

		egui::CollapsingHeader::new(name)
			.id_salt(ptr::from_ref(panel))
			.show(&mut *ui, |ui| {
				#[expect(clippy::match_same_arms, reason = "We'll be changing them soon")]
				match &mut panel.state {
					PanelState::None(_) => (),
					PanelState::Fade(state) =>
						self::draw_fade_panel_editor(ui, wgpu, window_geometry, state, &panel.geometries),
					PanelState::Slide(_) => (),
				}
			});
	}
}

/// Draws the fade panel editor
fn draw_fade_panel_editor(
	ui: &mut egui::Ui,
	wgpu: &Wgpu,
	window_geometry: Rect<i32, u32>,
	state: &mut PanelFadeState,
	geometries: &[PanelGeometry],
) {
	{
		let mut is_paused = state.is_paused();
		ui.checkbox(&mut is_paused, "Paused");
		state.set_paused(is_paused);
	}

	ui.collapsing("Geometries", |ui| {
		for (geometry_idx, panel_geometry) in geometries.iter().enumerate() {
			ui.horizontal(|ui| {
				let mut name = egui::WidgetText::from(format!("#{}: ", geometry_idx + 1));
				if !panel_geometry.intersects_window(window_geometry) {
					name = name.weak();
				}

				ui.label(name);
				super::draw_rect(ui, panel_geometry.geometry);
			});
		}
	});

	ui.horizontal(|ui| {
		ui.label("Cur progress");

		// Note: We only allow up until the duration - 1 so that you don't get stuck
		//       skipping images when you hold it at the max value
		// TODO: This max needs to be `duration - min_frame_duration` to not skip ahead.
		let max = state.duration().mul_f32(0.99);
		let mut progress = state.progress();
		super::draw_duration(ui, &mut progress, Duration::ZERO..=max);
		state.set_progress(progress);
	});

	ui.horizontal(|ui| {
		ui.label("Fade Duration");
		let min = Duration::ZERO;
		let max = state.duration() / 2;

		let mut fade_duration = state.fade_duration();
		super::draw_duration(ui, &mut fade_duration, min..=max);
		state.set_fade_duration(fade_duration);
	});

	ui.horizontal(|ui| {
		ui.label("Duration");

		let mut duration = state.duration();
		super::draw_duration(ui, &mut duration, Duration::ZERO..=Duration::from_secs_f32(180.0));
		state.set_duration(duration);
	});

	ui.horizontal(|ui| {
		ui.label("Skip");
		if ui.button("🔄").clicked() {
			state.skip(wgpu);
		}
	});

	ui.collapsing("Images", |ui| {
		self::draw_fade_panel_image(ui, "Previous", &mut state.images_mut().prev);
		self::draw_fade_panel_image(ui, "Current", &mut state.images_mut().cur);
		self::draw_fade_panel_image(ui, "Next", &mut state.images_mut().next);
	});
}

/// Draws a fade panel image
fn draw_fade_panel_image(ui: &mut egui::Ui, name: &str, image: &mut Option<PanelFadeImage>) {
	ui.horizontal(|ui| {
		ui.weak(name);

		let Some(image) = image else {
			ui.weak("[Unloaded]");
			return;
		};

		super::draw_openable_path(ui, &image.path);
		let texture = image.texture_view.texture();
		ui.label(format!("{}x{}", texture.width(), texture.height()));

		let swap_icon = match image.swap_dir {
			true => "⏪",
			false => "⏩",
		};
		if ui.button(swap_icon).clicked() {
			image.swap_dir.toggle();
		}
	});
}
