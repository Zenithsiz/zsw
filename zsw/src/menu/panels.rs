//! Panels tab

use {
	crate::panel::{Panel, Panels},
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
					Panel::Fade(shader) => shader.draw_editor(ui, wgpu, surface_geometry),
					Panel::Slide(_) => (),
				}
			});
	}
}
