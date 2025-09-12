//! Profiles tab

// Imports
use {crate::profile::Profiles, std::sync::Arc, zsw_util::TokioTaskBlockOn, zutil_cloned::cloned};

/// Draws the profiles tab
pub fn draw_profiles_tab(ui: &mut egui::Ui, profiles: &Arc<Profiles>) {
	for profile in profiles.get_all().block_on() {
		let profile = profile.lock().block_on();

		ui.collapsing(profile.name.to_string(), |ui| {
			#[expect(clippy::semicolon_if_nothing_returned, reason = "False positive")]
			if ui.button("Save").clicked() {
				let profile_name = profile.name.clone();

				#[cloned(profiles)]
				crate::spawn_task(format!("Save profile {:?}", profile.name), async move {
					profiles.save(&profile_name).await
				});
			}
		});
	}
}
