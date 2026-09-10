//! Project directories

use std::path::{Path, PathBuf};

/// Directories
#[derive(Debug)]
pub struct Dirs {
	/// Playlist directory
	pub playlists: PathBuf,

	/// Profiles directory
	pub profiles: PathBuf,
}

impl Dirs {
	/// Creates new directories from a few root paths
	pub fn new(config_dir: &Path) -> Self {
		Self {
			playlists: config_dir.join("playlists"),
			profiles:  config_dir.join("profiles"),
		}
	}
}
