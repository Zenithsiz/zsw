//! Profiles tab

// Imports
use {
	crate::{panel::Panels, playlist::Playlists, profile::Profiles},
	std::sync::{Arc, nonpoison::Mutex},
};

/// Draws the profiles tab
pub fn draw_profiles_tab(
	ui: &mut egui::Ui,
	playlists: &Arc<Playlists>,
	profiles: &Arc<Profiles>,
	panels: &Mutex<Panels>,
) {
	for (profile_name, profile) in &**profiles {
		if ui.button(profile_name.as_ref()).clicked() &&
			let Err(err) = panels.lock().set_profile(profile_name.clone(), profile, playlists)
		{
			tracing::warn!("Unable to set profile {profile_name}: {err:?}");
		}
	}
}
