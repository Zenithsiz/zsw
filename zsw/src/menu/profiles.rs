//! Profiles tab

// Imports
use {
	crate::{panel::Panels, playlist::Playlists, profile::Profiles},
	std::sync::Arc,
	zutil_cloned::cloned,
};

/// Draws the profiles tab
pub fn draw_profiles_tab(
	ui: &mut egui::Ui,
	playlists: &Arc<Playlists>,
	profiles: &Arc<Profiles>,
	panels: &Arc<Panels>,
) {
	for profile_name in profiles.names() {
		if ui.button(profile_name.as_ref()).clicked() {
			#[cloned(profile_name, playlists, profiles, panels;)]
			zsw_util::spawn_task(format!("Set profile active {profile_name:?}"), move || {
				panels.set_profile(&profile_name, &playlists, &profiles)
			});
		}
	}
}
