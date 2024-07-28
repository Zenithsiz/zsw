//! Logger

// Modules
pub mod pre_init;

// Imports
use {
	itertools::Itertools,
	std::{
		collections::{hash_map, HashMap},
		env::{self, VarError},
		fs,
		path::Path,
	},
	tracing::metadata::LevelFilter,
	tracing_subscriber::{prelude::*, EnvFilter},
};


/// Initializes the logger
///
/// Logs to both stderr and `log_file`, if any
pub fn init(log_file: Option<&Path>) {
	// Create the terminal layer
	let term_use_colors = self::colors_enabled();
	let term_env = self::get_env_filters("RUST_LOG", "info");
	let term_layer = tracing_subscriber::fmt::layer()
		.with_ansi(term_use_colors)
		.pretty()
		.with_filter(
			EnvFilter::builder()
				.with_default_directive(LevelFilter::INFO.into())
				.parse_lossy(term_env),
		);

	// Create the file layer, if requested
	let file_layer = log_file.and_then(|log_file| {
		// Try to create the file
		let file = match fs::File::create(log_file) {
			Ok(file) => file,
			Err(err) => {
				pre_init::warn(format!("Unable to create log file: {err}"));
				return None;
			},
		};

		// Then create the layer
		let env = self::get_env_filters("RUST_FILE_LOG", "debug");
		let layer = tracing_subscriber::fmt::layer()
			.with_writer(file)
			.with_ansi(false)
			.with_filter(EnvFilter::builder().parse_lossy(env));

		Some(layer)
	});

	// Register all layers to the registry
	let registry = tracing_subscriber::registry().with(term_layer).with(file_layer);

	#[cfg(feature = "tokio-console")]
	let registry = registry.with(console_subscriber::spawn());

	registry.init();
	tracing::debug!(?log_file, ?term_use_colors, "Initialized logging");

	// And emit all pre-init warnings
	for message in pre_init::take_traces() {
		tracing::trace!("{message}");
	}
	for message in pre_init::take_debugs() {
		tracing::debug!("{message}");
	}
	for message in pre_init::take_warnings() {
		tracing::warn!("{message}");
	}
}

/// Returns whether to colors should be enabled for the terminal layer.
// TODO: Check if we're being piped?
fn colors_enabled() -> bool {
	match env::var("RUST_LOG_COLOR").map(|var| var.to_lowercase()).as_deref() {
		// By default / `1` / `yes` / `true`, use colors
		Err(VarError::NotPresent) | Ok("1" | "yes" | "true") => true,

		// On `0`, `no`, `false`, don't
		Ok("0" | "no" | "false") => false,

		// Else don't use colors, but warn
		Ok(env) => {
			pre_init::warn(format!(
				"Ignoring unknown `RUST_LOG_COLOR` value: {env:?}, expected `0`, `1`, `yes`, `no`, `true`, `false`"
			));
			false
		},
		Err(VarError::NotUnicode(err)) => {
			pre_init::warn(format!("Ignoring non-utf8 `RUST_LOG_COLOR`: {err:?}"));
			false
		},
	}
}

/// Returns the env filters of a variable.
///
/// Adds default filters, if not specified
#[must_use]
fn get_env_filters(env: &str, default: &str) -> String {
	// Default filters
	let default_filters = [
		(None, default),
		(Some("wgpu"), "warn"),
		(Some("naga"), "warn"),
		(Some("winit"), "warn"),
		(Some("mio"), "warn"),
	];

	// Get the current filters
	let env_var;
	let mut cur_filters = match env::var(env) {
		// Split filters by `,`, then src and level by `=`
		Ok(var) => {
			env_var = var;
			env_var
				.split(',')
				.map(|s| match s.split_once('=') {
					Some((src, level)) => (Some(src), level),
					None => (None, s),
				})
				.collect::<HashMap<_, _>>()
		},

		// If there were none, don't use any
		Err(err) => {
			if let VarError::NotUnicode(var) = err {
				pre_init::warn(format!("Ignoring non-utf8 env variable {env:?}: {var:?}"));
			}

			HashMap::new()
		},
	};

	// Add all default filters, if not specified
	for (src, level) in default_filters {
		if let hash_map::Entry::Vacant(entry) = cur_filters.entry(src) {
			let _ = entry.insert(level);
		}
	}

	// Then re-create it
	let var = cur_filters
		.into_iter()
		.map(|(src, level)| match src {
			Some(src) => format!("{src}={level}"),
			None => level.to_owned(),
		})
		.join(",");
	pre_init::trace(format!("Using {env}={var}"));

	var
}
