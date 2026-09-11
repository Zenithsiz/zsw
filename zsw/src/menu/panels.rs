//! Panels tab

use {
	crate::panel::{Panel, Panels, shader},
	core::time::Duration,
	std::{ptr, sync::Arc},
	zsw_util::Rect,
	zsw_wgpu::Wgpu,
};

/// Draws the panels tab
pub fn draw_panels_tab(ui: &mut egui::Ui, wgpu: &Arc<Wgpu>, panels: &mut Panels, surface_geometry: Rect<i32, u32>) {
	self::draw_panels_editor(ui, wgpu, panels, surface_geometry);
	ui.separator();
}

/// Draws the panels editor
// TODO: Not edit the values as-is, as that breaks some invariants of panels (such as duration versus image states)
fn draw_panels_editor(ui: &mut egui::Ui, wgpu: &Arc<Wgpu>, panels: &mut Panels, surface_geometry: Rect<i32, u32>) {
	let panels = panels.get_all();
	if panels.is_empty() {
		ui.label("None loaded");
		return;
	}

	for (panel_idx, panel) in panels.iter_mut().enumerate() {
		let mut name = egui::WidgetText::from(format!("Panel #{panel_idx}"));
		if !panel.any_intersects(surface_geometry) {
			name = name.weak();
		}

		egui::CollapsingHeader::new(name)
			.id_salt(ptr::from_ref(panel))
			.show(&mut *ui, |ui| {
				#[expect(clippy::match_same_arms, reason = "We'll be changing them soon")]
				match panel {
					Panel::None(_) => (),
					Panel::Fade(shader) => self::draw_fade_panel_editor(ui, wgpu, surface_geometry, shader),
					Panel::Slide(_) => (),
				}
			});
	}
}

/// Draws the fade panel editor
fn draw_fade_panel_editor(
	ui: &mut egui::Ui,
	wgpu: &Arc<Wgpu>,
	surface_geometry: Rect<i32, u32>,
	shader: &mut shader::fade::Shader,
) {
	{
		let mut is_paused = shader.is_paused();
		ui.checkbox(&mut is_paused, "Paused");
		shader.set_paused(is_paused);
	}

	ui.collapsing("Geometries", |ui| {
		for (geometry_idx, panel_geometry) in shader.geometries().iter().enumerate() {
			ui.horizontal(|ui| {
				let mut name = egui::WidgetText::from(format!("#{}: ", geometry_idx + 1));
				if !panel_geometry.rect.intersects(surface_geometry) {
					name = name.weak();
				}

				ui.label(name);
				super::draw_rect(ui, panel_geometry.rect);
			});
		}
	});

	ui.horizontal(|ui| {
		ui.label("Cur progress");

		// Note: We only allow up until the duration - 1 so that you don't get stuck
		//       skipping images when you hold it at the max value
		// TODO: This max needs to be `duration - min_frame_duration` to not skip ahead.
		let max = shader.duration().mul_f32(0.99);
		let mut progress = shader.progress();
		super::draw_duration(ui, &mut progress, Duration::ZERO..=max);
		shader.set_progress(progress);
	});

	ui.horizontal(|ui| {
		ui.label("Fade Duration");
		let min = Duration::ZERO;
		let max = shader.duration() / 2;

		let mut fade_duration = shader.fade_duration();
		super::draw_duration(ui, &mut fade_duration, min..=max);
		shader.set_fade_duration(fade_duration);
	});

	ui.horizontal(|ui| {
		ui.label("Duration");

		let mut duration = shader.duration();
		super::draw_duration(ui, &mut duration, Duration::ZERO..=Duration::from_secs_f32(180.0));
		shader.set_duration(duration);
	});

	ui.horizontal(|ui| {
		ui.label("Skip");
		if ui.button("🔄").clicked() {
			shader.skip(wgpu);
		}
	});

	ui.collapsing("Images", |ui| {
		self::draw_fade_panel_image(ui, "Previous", &mut shader.images_mut().prev);
		self::draw_fade_panel_image(ui, "Current", &mut shader.images_mut().cur);
		self::draw_fade_panel_image(ui, "Next", &mut shader.images_mut().next);
	});
}

/// Draws a fade panel image
fn draw_fade_panel_image(ui: &mut egui::Ui, name: &str, image: &mut Option<shader::fade::Image>) {
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
