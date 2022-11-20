//! Profile

// Imports
use {
	std::path::PathBuf,
	zsw_panels::{Panel, Panels, PanelsResource},
	zsw_playlist::PlaylistManager,
};

/// A profile
#[derive(Clone, Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Profile {
	/// Root path
	pub root_path: PathBuf,

	/// All panels
	pub panels: Vec<Panel>,
}

impl Profile {
	/// Applies this profile
	pub fn apply<'panels>(
		&self,
		playlist_manager: &PlaylistManager,
		panels: &'panels Panels,
		panels_resource: &mut PanelsResource,
	) {
		// TODO: Also flush out all image buffers
		tracing::debug!("Applying profile");
		playlist_manager.set_root_path(self.root_path.clone());
		panels.replace_panels(panels_resource, self.panels.iter().copied());
	}
}
