//! App configuration

// Imports
use std::path::PathBuf;

/// App configuration
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Config {
	/// Profiles
	///
	/// First one is treated as the default profile
	pub profiles: Vec<PathBuf>,
}

#[allow(clippy::derivable_impls)] // Better to be explicit with a config
impl Default for Config {
	fn default() -> Self {
		Self { profiles: vec![] }
	}
}
