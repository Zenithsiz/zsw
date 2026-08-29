//! Profiles tab

use crate::{panel::Panels, playlist::Playlists, profile::Profiles};

/// Draws the profiles tab
pub fn draw_profiles_tab(ui: &mut egui::Ui, playlists: &Playlists, profiles: &Profiles, panels: &mut Panels) {
	for (profile_name, profile) in profiles {
		if ui.button(profile_name.as_ref()).clicked() &&
			let Err(err) = panels.set_profile(profile_name.clone(), profile, playlists)
		{
			tracing::warn!("Unable to set profile {profile_name}: {err:?}");
		}
	}
}
