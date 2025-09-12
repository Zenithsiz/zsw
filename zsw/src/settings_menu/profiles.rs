//! Profiles tab

// Imports
use {
	crate::{display::Displays, panel::Panels, playlist::Playlists, profile::Profiles},
	std::sync::Arc,
	zsw_util::TokioTaskBlockOn,
	zutil_cloned::cloned,
};

/// Draws the profiles tab
pub fn draw_profiles_tab(
	ui: &mut egui::Ui,
	displays: &Arc<Displays>,
	playlists: &Arc<Playlists>,
	profiles: &Arc<Profiles>,
	panels: &Arc<Panels>,
) {
	for profile in profiles.get_all().block_on() {
		let profile = profile.lock().block_on();

		ui.collapsing(profile.name.to_string(), |ui| {
			if ui.button("Set active").clicked() {
				#[cloned(profile_name = profile.name, displays, playlists, profiles, panels;)]
				crate::spawn_task(format!("Set profile active {profile_name:?}"), async move {
					panels.set_profile(profile_name, &displays, &playlists, &profiles).await
				});
			}

			if ui.button("Save").clicked() {
				#[cloned(profiles, profile_name = profile.name;)]
				crate::spawn_task(format!("Save profile {profile_name:?}"), async move {
					profiles.save(&profile_name).await
				});
			}
		});
	}
}
