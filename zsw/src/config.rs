//! Configuration

// Imports
use {
	anyhow::Context,
	std::{
		collections::HashSet,
		num::NonZeroUsize,
		path::{Path, PathBuf},
	},
	zsw_error::AppError,
};

/// Configuration
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Config {
	/// Tokio worker threads
	#[serde(default)]
	pub tokio_worker_threads: Option<NonZeroUsize>,

	/// Rayon worker threads
	#[serde(default)]
	pub rayon_worker_threads: Option<NonZeroUsize>,

	/// Default config file.
	///
	/// Will be overridden by command-line arguments
	#[serde(default)]
	pub log_file: Option<PathBuf>,

	/// Panels directory
	#[serde(default)]
	pub panels_dir: Option<PathBuf>,

	/// Playlists directory
	#[serde(default)]
	pub playlists_dir: Option<PathBuf>,

	/// Shaders directory
	#[serde(default)]
	pub shaders_dir: Option<PathBuf>,

	/// Upscale cache directory
	#[serde(default)]
	pub upscale_cache_dir: Option<PathBuf>,

	/// Upscaling command, if any.
	///
	/// Will be called with arguments `["-i", <input-file>, "-o", <output-file>, "-s", <integer-power-of-two-scale>]`
	#[serde(default)]
	pub upscale_cmd: Option<PathBuf>,

	/// Upscaling excluded (by absolute path)
	#[serde(default)]
	pub upscale_exclude: HashSet<PathBuf>,

	/// Default panel group
	#[serde(default)]
	pub default_panel_group: Option<String>,
}

impl Config {
	/// Tries to load the config
	///
	/// If unable to, attempts to create a default config
	pub fn get_or_create_default(path: &Path) -> Self {
		match Self::load(path) {
			Ok(config) => config,
			Err(err) => {
				tracing::warn!("Unable to load config, using default: {err:?}");
				let config = Self::default();

				// If the config file doesn't exist, write the default
				// Note: If we're unable to check for existence, we assume it does exist, so we don't override anything
				if !std::fs::try_exists(path).unwrap_or(true) {
					if let Err(err) = config.write(path) {
						tracing::warn!("Unable to write default config: {err:?}");
					}
				}

				config
			},
		}
	}

	/// Loads the config
	fn load(path: &Path) -> Result<Self, AppError> {
		tracing::debug!(?path, "Loading config");

		let config_yaml = std::fs::read(path).context("Unable to open file")?;
		let config = serde_yaml::from_slice(&config_yaml).context("Unable to parse config")?;
		Ok(config)
	}

	/// Writes the config
	fn write(&self, path: &Path) -> Result<(), AppError> {
		let config_yaml = serde_yaml::to_string(self).context("Unable to serialize config")?;
		std::fs::write(path, config_yaml.as_bytes()).context("Unable to write config")?;

		Ok(())
	}
}

#[expect(clippy::derivable_impls)] // we want to be explicit with defaults
impl Default for Config {
	fn default() -> Self {
		Self {
			tokio_worker_threads: None,
			rayon_worker_threads: None,
			log_file:             None,
			panels_dir:           None,
			playlists_dir:        None,
			shaders_dir:          None,
			upscale_cache_dir:    None,
			upscale_cmd:          None,
			upscale_exclude:      HashSet::new(),
			default_panel_group:  None,
		}
	}
}
