//! Configuration directories

// Imports
use std::{
	path::{Path, PathBuf},
	sync::OnceLock,
};

/// Config directories
#[derive(Debug)]
pub struct ConfigDirs {
	/// Root config directory
	root: PathBuf,

	/// Panels directory
	panels: OnceLock<PathBuf>,

	/// Playlist directory
	playlists: OnceLock<PathBuf>,

	/// Profiles directory
	profiles: OnceLock<PathBuf>,
}

impl ConfigDirs {
	/// Creates new config dirs from the root path
	pub fn new(root: PathBuf) -> Self {
		Self {
			root,
			panels: OnceLock::new(),
			playlists: OnceLock::new(),
			profiles: OnceLock::new(),
		}
	}

	/// Returns the panels directory
	pub fn panels(&self) -> &Path {
		self.panels.get_or_init(|| {
			let path = self.root.join("panels");
			tracing::info!("Panels path: {path:?}");
			path
		})
	}

	/// Returns the playlists directory
	pub fn playlists(&self) -> &Path {
		self.playlists.get_or_init(|| {
			let path = self.root.join("playlists");
			tracing::info!("Playlists path: {path:?}");
			path
		})
	}

	/// Returns the profiles directory
	pub fn profiles(&self) -> &Path {
		self.profiles.get_or_init(|| {
			let path = self.root.join("profiles");
			tracing::info!("Playlists path: {path:?}");
			path
		})
	}
}
