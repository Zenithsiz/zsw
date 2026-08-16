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
		let profile = profile.read().block_on();

		if ui.button(profile.name.as_ref()).clicked() {
			#[cloned(profile_name = profile.name, displays, playlists, profiles, panels;)]
			zsw_util::spawn_task(format!("Set profile active {profile_name:?}"), async move {
				panels.set_profile(profile_name, &displays, &playlists, &profiles).await
			});
		}
	}
}
