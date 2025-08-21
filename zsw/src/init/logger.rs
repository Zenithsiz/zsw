//! Logger

// Imports
use {
	itertools::Itertools,
	std::{
		collections::{HashMap, hash_map},
		env::{self, VarError},
		fs,
		io::{self, IsTerminal},
		path::Path,
	},
	tracing::metadata::LevelFilter,
	tracing_subscriber::{EnvFilter, fmt::format::FmtSpan, prelude::*},
};


/// Initializes the logger
///
/// Logs to both stderr and `log_file`, if any
pub fn init(log_file: Option<&Path>) -> tracing::dispatcher::DefaultGuard {
	// Create the terminal layer
	let term_use_colors = self::colors_enabled();
	let term_env = self::get_env_filters("RUST_LOG", "info");
	let term_layer = tracing_subscriber::fmt::layer()
		.with_span_events(FmtSpan::CLOSE)
		.with_ansi(term_use_colors);
	#[cfg(debug_assertions)]
	let term_layer = term_layer
		.with_file(true)
		.with_line_number(true)
		.with_thread_names(true);
	let term_layer = term_layer.with_filter(
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
				tracing::warn!("Unable to create log file: {err}");
				return None;
			},
		};

		// Then create the layer
		let env = self::get_env_filters("RUST_FILE_LOG", "debug");
		let layer = tracing_subscriber::fmt::layer()
			.with_span_events(FmtSpan::CLOSE)
			.with_writer(file)
			.with_ansi(false)
			.with_filter(EnvFilter::builder().parse_lossy(env));

		Some(layer)
	});

	// Register all layers to the registry
	let registry = tracing_subscriber::registry().with(term_layer).with(file_layer);

	#[cfg(feature = "tokio-console")]
	let registry = registry.with(console_subscriber::spawn());

	let guard = registry.set_default();
	tracing::debug!(?log_file, ?term_use_colors, "Initialized logging");

	guard
}

/// Returns whether to colors should be enabled for the terminal layer.
fn colors_enabled() -> bool {
	// If `NO_COLOR` is set to non-empty, we shouldn't use colors
	if env::var("NO_COLOR").is_ok_and(|var| !var.is_empty()) {
		return false;
	}

	// Otherwise, enable colors if we're not being piped
	io::stdout().is_terminal()
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
				tracing::warn!("Ignoring non-utf8 env variable {env:?}: {var:?}");
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
	tracing::trace!("Using {env}={var}");

	var
}
