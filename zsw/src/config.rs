//! App configuration

// Imports
use std::{num::NonZeroUsize, path::PathBuf};

/// App configuration
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Config {
	/// Profiles
	///
	/// First one successfully loaded is treated as the default profile
	#[serde(default)]
	pub profiles: Vec<PathBuf>,

	// TODO: Move the thread count to profiles?
	/// Image loader threads
	#[serde(default)]
	pub image_loader_threads: Option<NonZeroUsize>,

	/// Image resizer threads
	#[serde(default)]
	pub image_resizer_threads: Option<NonZeroUsize>,
}

#[allow(clippy::derivable_impls)] // Better to be explicit with a config
impl Default for Config {
	fn default() -> Self {
		Self {
			profiles:              vec![],
			image_loader_threads:  None,
			image_resizer_threads: None,
		}
	}
}
