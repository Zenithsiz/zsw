//! Configuration

// Imports
use {
	app_error::Context,
	std::{
		fs,
		num::NonZeroUsize,
		path::{Path, PathBuf},
	},
	zsw_util::AppError,
};

/// Configuration
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Config {
	/// Tokio worker threads
	#[serde(default)]
	pub tokio_worker_threads: Option<NonZeroUsize>,

	/// Default config file.
	///
	/// Will be overridden by command-line arguments
	#[serde(default)]
	pub log_file: Option<PathBuf>,

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
				tracing::warn!("Unable to load config from {path:?}, using default: {}", err.pretty());
				let config = Self::default();

				// If the config file doesn't exist, write the default
				// Note: If we're unable to check for existence, we assume it does exist, so we don't override anything
				if !fs::exists(path).unwrap_or(true) &&
					let Err(err) = config.write(path)
				{
					tracing::warn!("Unable to write default config to {path:?}: {}", err.pretty());
				}

				config
			},
		}
	}

	/// Loads the config
	fn load(path: &Path) -> Result<Self, AppError> {
		tracing::debug!("Loading config from path: {path:?}");

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
			log_file:             None,
			default:              ConfigDefault::default(),
		}
	}
}

/// Config default
#[derive(Clone, Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ConfigDefault {
	/// Profile
	pub profile: Option<String>,
}

#[expect(clippy::derivable_impls, reason = "We want to be explicit with defaults")]
impl Default for ConfigDefault {
	fn default() -> Self {
		Self { profile: None }
	}
}
