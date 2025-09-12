//! Displays tab

// Imports
use {
	crate::display::{Display, DisplayName, Displays},
	std::sync::Arc,
	zsw_util::{Rect, TokioTaskBlockOn},
	zutil_cloned::cloned,
};

/// Draws the displays tab
pub fn draw_displays_tab(ui: &mut egui::Ui, displays: &Arc<Displays>) {
	for display in displays.get_all().block_on() {
		let mut display = display.lock().block_on();
		ui.collapsing(display.name.to_string(), |ui| {
			self::draw_display_geometries(ui, &mut display.geometries);

			if ui.button("Save").clicked() {
				#[cloned(displays, display_name = display.name;)]
				zsw_util::spawn_task(format!("Save display {display_name:?}"), async move {
					displays.save(&display_name).await
				});
			}
		});
	}

	ui.separator();

	ui.collapsing("New", |ui| {
		let name = super::get_data::<String>(ui, "display-tab-new-name");
		let geometries = super::get_data::<Vec<Rect<i32, u32>>>(ui, "display-tab-new-geometries");

		ui.horizontal(|ui| {
			ui.label("Name");
			ui.text_edit_singleline(&mut *name.lock());
		});
		self::draw_display_geometries(ui, &mut geometries.lock());


		if ui.button("Add").clicked() {
			let display_name = DisplayName::from(name.lock().clone());

			#[cloned(displays, geometries = geometries.lock();)]
			zsw_util::spawn_task(format!("Add display {display_name:?}"), async move {
				// TODO: Should we also save it?
				displays
					.add(display_name.clone(), Display {
						name: display_name,
						geometries,
					})
					.await;

				Ok(())
			});
		}
	});
}


/// Draws a display's geometries
pub fn draw_display_geometries(ui: &mut egui::Ui, geometries: &mut Vec<Rect<i32, u32>>) {
	let mut geometry_idx = 0;
	geometries.retain_mut(|geometry| {
		let mut retain = true;
		ui.horizontal(|ui| {
			ui.label(format!("#{}: ", geometry_idx + 1));
			super::draw_rect(ui, geometry);
			if ui.button("-").clicked() {
				retain = false;
			}
		});

		geometry_idx += 1;
		retain
	});

	if ui.button("+").clicked() {
		geometries.push(Rect::zero());
	}
}
