//! Displays tab

// Imports
use {
	crate::display::{Display, Displays},
	std::sync::Arc,
	zsw_util::TokioTaskBlockOn,
	zutil_cloned::cloned,
};

/// Draws the displays tab
pub fn draw_displays_tab(ui: &mut egui::Ui, displays: &Arc<Displays>) {
	for display in displays.get_all().block_on() {
		let mut display = display.lock().block_on();
		ui.collapsing(display.name.to_string(), |ui| {
			self::draw_display(ui, &mut display);

			if ui.button("Save").clicked() {
				let display_name = display.name.clone();

				#[cloned(displays)]
				if let Err(err) = crate::spawn_task(format!("Save display {:?}", display.name), async move {
					displays.save(&display_name).await
				}) {
					tracing::warn!("Unable to spawn task: {}", err.pretty());
				}
			}
		});
	}
}

/// Draws a display
pub fn draw_display(ui: &mut egui::Ui, display: &mut Display) {
	for (geometry_idx, geometry) in display.geometries.iter_mut().enumerate() {
		ui.horizontal(|ui| {
			ui.label(format!("#{}: ", geometry_idx + 1));
			super::draw_rect(ui, geometry);
		});
	}
}
