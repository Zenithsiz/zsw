//! Zenithsiz's scrolling wallpaper

// Features
#![feature(
	never_type,
	control_flow_enum,
	decl_macro,
	inline_const,
	stmt_expr_attributes,
	try_trait_v2,
	thread_id_value,
	unwrap_infallible,
	async_closure,
	generators,
	generator_trait,
	associated_type_bounds,
	let_chains,
	type_alias_impl_trait
)]

// Modules
mod app;
mod args;
mod config;
mod trace;

// Exports
pub use self::{args::Args, config::Config};

// Imports
use {
	anyhow::Context,
	clap::Parser,
	directories::ProjectDirs,
	std::{
		fs,
		io,
		num::NonZeroUsize,
		path::Path,
		sync::{
			atomic::{self, AtomicUsize},
			Arc,
		},
		thread,
	},
};

fn main() -> Result<(), anyhow::Error> {
	// Initialize tracing
	trace::init();

	// Get arguments
	let args = match Args::try_parse() {
		Ok(args) => args,
		Err(err) => {
			tracing::warn!(?err, "Unable to retrieve arguments");
			err.exit();
		},
	};
	tracing::debug!(?args, "Arguments");

	// Try to create the directories for the app
	let dirs = ProjectDirs::from("", "", "zsw").context("Unable to create app directories")?;
	fs::create_dir_all(dirs.data_dir()).context("Unable to create data directory")?;

	// Then read the config
	let config_path = args.config.unwrap_or_else(|| dirs.data_dir().join("config.yaml"));
	tracing::debug!("Loading config from {config_path:?}");
	let config = self::load_config_or_default(&config_path);
	tracing::debug!(?config, "Loaded config");
	let config = Arc::new(config);

	// Customize the rayon pool thread
	// Note: This is used indirectly in `image` by `jpeg-decoder`
	rayon::ThreadPoolBuilder::new()
		.thread_name(|idx| format!("rayon${idx}"))
		.num_threads(
			config
				.rayon_threads
				.or_else(|| thread::available_parallelism().ok())
				.map_or(1, NonZeroUsize::get),
		)
		.build_global()
		.context("Unable to build `rayon` global thread pool")?;

	// Create the runtime and enter it
	let runtime = self::create_runtime(&config)?;
	let _runtime_enter = runtime.enter();

	// Run the app and restart if we get an error (up to 5 errors)
	let mut errors = 0;
	while errors < 5 {
		match runtime.block_on(app::run(&dirs, &config)) {
			Ok(()) => {
				tracing::info!("Application finished");
				break;
			},
			Err(err) => {
				tracing::error!(?err, "Application encountered fatal error");
				errors += 1;
				continue;
			},
		}
	}

	Ok(())
}

/// Creates the tokio runtime
fn create_runtime(config: &Config) -> Result<tokio::runtime::Runtime, anyhow::Error> {
	tokio::runtime::Builder::new_multi_thread()
		.enable_time()
		.thread_name_fn(|| {
			static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
			let id = NEXT_ID.fetch_add(1, atomic::Ordering::AcqRel);
			format!("tokio${id}")
		})
		.worker_threads(
			config
				.tokio_threads
				.or_else(|| thread::available_parallelism().ok())
				.map_or(1, NonZeroUsize::get),
		)
		.build()
		.context("Unable to create runtime")
}

/// Loads the config or default
fn load_config_or_default(config_path: &Path) -> Config {
	match self::load_config(config_path) {
		Ok(config) => config,
		Err(err) if err.is_open_file() => {
			tracing::debug!("No config file found, creating a default");
			let config = Config::default();
			if let Err(err) = self::write_config(config_path, &config) {
				tracing::warn!("Unable to write default config: {err:?}");
			}
			config
		},
		Err(err) => {
			tracing::warn!("Unable to open config file, using default: {err:?}");
			Config::default()
		},
	}
}

#[derive(Debug, thiserror::Error)]
enum LoadError {
	/// Open file
	#[error("Unable to open file")]
	OpenFile(#[source] io::Error),

	/// Read file
	#[error("Unable to read file")]
	ReadFile(#[source] serde_yaml::Error),
}

impl LoadError {
	/// Returns `true` if the load error is [`OpenFile`].
	///
	/// [`OpenFile`]: LoadError::OpenFile
	#[must_use]
	fn is_open_file(&self) -> bool {
		matches!(self, Self::OpenFile(_))
	}
}

/// Loads the config.
fn load_config(config_path: &Path) -> Result<Config, LoadError> {
	let config_file = fs::File::open(config_path).map_err(LoadError::OpenFile)?;
	let config = serde_yaml::from_reader(config_file).map_err(LoadError::ReadFile)?;

	Ok(config)
}

/// Writes the config
fn write_config(config_path: &Path, config: &Config) -> Result<(), anyhow::Error> {
	let config_file = fs::File::create(config_path)?;
	serde_yaml::to_writer(config_file, config)?;
	Ok(())
}
