//! Configuration directories

// Imports
use std::{
	path::{Path, PathBuf},
	sync::OnceLock,
};

/// Config directories
pub struct ConfigDirs {
	/// Root config directory
	root: PathBuf,

	/// Panels directory
	panels: OnceLock<PathBuf>,

	/// Playlist directory
	playlists: OnceLock<PathBuf>,

	/// Shaders directory
	shaders: OnceLock<PathBuf>,
}

impl ConfigDirs {
	/// Creates new config dirs from the root path
	pub fn new(root: PathBuf) -> Self {
		Self {
			root,
			panels: OnceLock::new(),
			playlists: OnceLock::new(),
			shaders: OnceLock::new(),
		}
	}

	/// Returns the panels directory
	pub fn panels(&self) -> &Path {
		self.panels.get_or_init(|| {
			let path = self.root.join("panels");
			tracing::info!(?path, "Panels path");
			path
		})
	}

	/// Returns the playlists directory
	pub fn playlists(&self) -> &Path {
		self.playlists.get_or_init(|| {
			let path = self.root.join("playlists");
			tracing::info!(?path, "Playlists path");
			path
		})
	}

	/// Returns the shaders directory
	pub fn shaders(&self) -> &Path {
		self.shaders.get_or_init(|| {
			let path = self.root.join("shaders");
			tracing::info!(?path, "Shaders path");
			path
		})
	}
}
