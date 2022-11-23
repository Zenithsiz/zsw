//! Profile applier

// Imports
use super::Services;

/// Profile applier
#[derive(Clone, Debug)]
pub struct ProfileApplier {}

impl ProfileApplier {
	/// Creates a new profile applier
	pub fn new() -> Self {
		Self {}
	}
}

impl zsw_settings_window::ProfileApplier<Services> for ProfileApplier {
	fn apply(
		&self,
		profile: &zsw_profiles::Profile,
		services: &Services,
		panels_resource: &mut zsw_panels::PanelsResource,
	) {
		// TODO: Also flush out all image buffers
		tracing::debug!(?profile, "Applying profile");
		services.playlist_manager.set_root_path(profile.root_path.clone());
		services
			.panels
			.replace_panels(panels_resource, profile.panels.iter().copied());
	}
}
