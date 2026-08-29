//! Profiles tab

use crate::{panel::Panels, shared::Shared};

/// Draws the profiles tab
pub fn draw_profiles_tab(ui: &mut egui::Ui, shared: &Shared, panels: &mut Panels) {
	for (profile_name, profile) in &*shared.profiles {
		if ui.button(profile_name.as_ref()).clicked() &&
			let Err(err) = panels.set_profile(profile_name.clone(), profile, &shared.playlists)
		{
			tracing::warn!("Unable to set profile {profile_name}: {err:?}");
		}
	}
}
