//! Profile applier

// Imports
use {super::Services, std::path::PathBuf, zsw_profiles::Profile};

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

	fn current(&self, services: &Services, panels_resource: &mut zsw_panels::PanelsResource) -> zsw_profiles::Profile {
		Profile {
			root_path: match services.playlist_manager.root_path() {
				Some(path) => path,
				// TODO: What to do here?
				None => {
					tracing::warn!("No root path was set");
					PathBuf::from("<not set>")
				},
			},
			panels:    services
				.panels
				.panels(panels_resource)
				.iter()
				.map(|panel| panel.panel)
				.collect(),
		}
	}
}
