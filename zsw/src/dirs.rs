//! Project directories

// Imports
use std::{
	path::{Path, PathBuf},
	sync::OnceLock,
};

/// Directories
#[derive(Debug)]
pub struct Dirs {
	/// Root config directory
	config_dir: PathBuf,

	/// Displays directory
	displays: OnceLock<PathBuf>,

	/// Playlist directory
	playlists: OnceLock<PathBuf>,

	/// Profiles directory
	profiles: OnceLock<PathBuf>,
}

impl Dirs {
	/// Creates new directories from a few root paths
	pub fn new(config_dir: PathBuf) -> Self {
		Self {
			config_dir,
			displays: OnceLock::new(),
			playlists: OnceLock::new(),
			profiles: OnceLock::new(),
		}
	}

	/// Returns the displays directory
	pub fn displays(&self) -> &Path {
		self.displays.get_or_init(|| {
			let path = self.config_dir.join("displays");
			tracing::info!("Panels path: {path:?}");
			path
		})
	}

	/// Returns the playlists directory
	pub fn playlists(&self) -> &Path {
		self.playlists.get_or_init(|| {
			let path = self.config_dir.join("playlists");
			tracing::info!("Playlists path: {path:?}");
			path
		})
	}

	/// Returns the profiles directory
	pub fn profiles(&self) -> &Path {
		self.profiles.get_or_init(|| {
			let path = self.config_dir.join("profiles");
			tracing::info!("Playlists path: {path:?}");
			path
		})
	}
}
