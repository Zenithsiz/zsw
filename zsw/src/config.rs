//! Configuration

// Imports
use {
	std::{
		collections::HashSet,
		fs,
		num::NonZeroUsize,
		path::{Path, PathBuf},
	},
	zutil_app_error::{AppError, Context},
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

	/// Default
	#[serde(default)]
	pub default: ConfigDefault,
}

impl Config {
	/// Tries to load the config
	///
	/// If unable to, attempts to create a default config
	pub fn get_or_create_default(path: &Path) -> Self {
		match Self::load(path) {
			Ok(config) => config,
			Err(err) => {
				logger::pre_init::warn(format!("Unable to load config from {path:?}, using default: {err:?}"));
				let config = Self::default();

				// If the config file doesn't exist, write the default
				// Note: If we're unable to check for existence, we assume it does exist, so we don't override anything
				if !fs::exists(path).unwrap_or(true) &&
					let Err(err) = config.write(path)
				{
					logger::pre_init::warn(format!("Unable to write default config to {path:?}: {err:?}"));
				}

				config
			},
		}
	}

	/// Loads the config
	fn load(path: &Path) -> Result<Self, AppError> {
		logger::pre_init::debug(format!("Loading config from path: {path:?}"));

		let config_toml = fs::read_to_string(path).context("Unable to open file")?;
		let config = toml::from_str(&config_toml).context("Unable to parse config")?;
		Ok(config)
	}

	/// Writes the config
	fn write(&self, path: &Path) -> Result<(), AppError> {
		let config_toml = toml::to_string(self).context("Unable to serialize config")?;
		fs::write(path, config_toml.as_bytes()).context("Unable to write config")?;

		Ok(())
	}
}

#[expect(clippy::derivable_impls, reason = "We want to be explicit with defaults")]
impl Default for Config {
	fn default() -> Self {
		Self {
			tokio_worker_threads: None,
			rayon_worker_threads: None,
			log_file:             None,
			upscale_cache_dir:    None,
			upscale_cmd:          None,
			upscale_exclude:      HashSet::new(),
			default:              ConfigDefault::default(),
		}
	}
}

/// Config default
#[derive(Clone, Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ConfigDefault {
	/// Panels
	pub panels: Vec<ConfigPanel>,
}

#[expect(clippy::derivable_impls, reason = "We want to be explicit with defaults")]
impl Default for ConfigDefault {
	fn default() -> Self {
		Self { panels: vec![] }
	}
}

/// Configuration panel
#[derive(Clone, Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ConfigPanel {
	/// Panel
	pub panel: String,

	/// Playlist
	pub playlist: String,
}
