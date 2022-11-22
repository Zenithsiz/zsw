//! Tracing

// Imports
use {
	std::{backtrace::Backtrace, fs, io, panic, thread},
	tracing::{metadata::LevelFilter, Level, Subscriber},
	tracing_subscriber::{
		filter,
		fmt::{self, format::FmtSpan},
		layer::{self, Layer},
		prelude::*,
		registry::LookupSpan,
	},
};

/// Initializes tracing
pub fn init() {
	// Create the base registry
	let registry = tracing_subscriber::registry();

	// Initialize file and console fmt layer
	let registry = registry.with(self::console_fmt_layer());
	let registry = registry.with(self::file_fmt_layer());

	// Initialize tokio console layer if requested
	#[cfg(feature = "tokio-console")]
	let registry = registry.with(console_subscriber::spawn());

	// Finally initialize the registry
	registry.init();

	// Then replace the panic hook
	panic::set_hook(Box::new(self::panic_hook));
}

/// File fmt layer
fn file_fmt_layer<S>() -> impl Layer<S>
where
	S: Subscriber + for<'a> LookupSpan<'a>,
{
	// Setup the format
	let format = fmt::format()
		.with_ansi(false)
		.with_file(false)
		.with_line_number(false)
		.with_thread_names(false)
		.with_thread_ids(false);

	// Setup the target filter
	let target_filter = self::target_filters(LevelFilter::DEBUG);

	// TODO: Not panic if unable to create log file
	// TODO: Not leak the file
	let file = fs::File::create("latest.log").expect("Unable to create logger file");
	let file: &'static _ = Box::leak(Box::new(file));
	fmt::layer()
		.event_format(format)
		.with_span_events(FmtSpan::NONE)
		.with_writer(move || file)
		.with_filter(target_filter)
}

/// Console fmt layer
fn console_fmt_layer<S>() -> impl Layer<S>
where
	S: Subscriber + for<'a> LookupSpan<'a>,
{
	// Setup the format
	let format = fmt::format()
		.pretty()
		.compact()
		.with_file(false)
		.with_line_number(false)
		.with_thread_names(false)
		.with_thread_ids(false);

	// Setup the target filter
	let target_filter = self::target_filters(LevelFilter::INFO);

	fmt::layer()
		.event_format(format)
		.with_span_events(FmtSpan::NONE)
		.with_writer(io::stderr)
		.with_filter(target_filter)
}

/// Create targets filters with a default level
fn target_filters<S>(default_level: LevelFilter) -> Box<dyn layer::Filter<S> + Send + Sync + 'static>
where
	S: Subscriber + for<'a> LookupSpan<'a>,
{
	match tracing_subscriber::EnvFilter::builder()
		.with_default_directive(default_level.into())
		.try_from_env()
	{
		// TODO: Still apply the usuals filters by default, unless the user explicitly changes them?
		Ok(env_filter) => Box::new(env_filter) as Box<dyn layer::Filter<S> + Send + Sync + 'static>,
		Err(_) => Box::new(self::add_filters(filter::Targets::new().with_default(default_level))),
	}
}

/// Adds filters to a targets
fn add_filters(filter: filter::Targets) -> filter::Targets {
	filter
		.with_target("wgpu", Level::WARN)
		.with_target("wgpu_hal", Level::WARN)
		.with_target("wgpu_core", Level::WARN)
		.with_target("naga", Level::WARN)
		.with_target("winit", Level::WARN)
		.with_target("mio", Level::WARN)
}

/// Panic hook
#[track_caller]
fn panic_hook(info: &panic::PanicInfo<'_>) {
	let location = info.location().expect("Panic had no location");
	let msg = match info.payload().downcast_ref::<&'static str>() {
		Some(s) => *s,
		None => match info.payload().downcast_ref::<String>() {
			Some(s) => s.as_str(),
			None => "Box<dyn Any>",
		},
	};
	let thread = thread::current();
	let thread_name = thread.name().unwrap_or("<unnamed>");

	let backtrace = Backtrace::force_capture();

	tracing::error!(
		thread_name,
		msg,
		?location,
		?backtrace,
		"Thread '{thread_name}' panicked at '{msg}'"
	);
}
