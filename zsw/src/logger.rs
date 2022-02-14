//! Logger

// Imports
use {
	anyhow::Context,
	fern::colors::{Color, ColoredLevelConfig},
	std::{fmt, fs, panic, thread},
	zsw_util::DisplayWrapper,
};

// Time format string
const TIME_FMT: &str = "%Y-%m-%d %H:%M:%S";

/// Record colors
const RECORD_COLORS: ColoredLevelConfig = ColoredLevelConfig {
	error: Color::BrightRed,
	warn:  Color::Red,
	info:  Color::Blue,
	debug: Color::Yellow,
	trace: Color::White,
};

/// Initializes all logging for the application
pub fn init() -> Result<(), anyhow::Error> {
	// Create the base dispatch
	let mut dispatch = fern::Dispatch::new().chain(self::stderr_dispatch());

	// Try to add a file dispatch
	let file_err = match self::file_dispatch() {
		Ok(file) => {
			dispatch = dispatch.chain(file);
			None
		},
		Err(err) => Some(err),
	};

	// Then apply
	dispatch.apply().context("Unable to initialize logger")?;

	// If we failed to add the file dispatch, log a warning
	if let Some(err) = file_err {
		log::warn!("Unable to create file logger: {err:?}");
	}

	// Finally set the panic hook to log errors
	panic::set_hook(Box::new(self::panic_hook));

	Ok(())
}

/// Returns the stderr dispatch
fn stderr_dispatch() -> fern::Dispatch {
	let dispatch = fern::Dispatch::new()
		.format(self::fmt_log(true))
		.level(log::LevelFilter::Info)
		.chain(std::io::stderr());

	// Note: For `stderr` always only emit warnings from other libraries
	self::set_libs_levels(dispatch, log::LevelFilter::Warn)
}

/// Returns the file dispatch
fn file_dispatch() -> Result<fern::Dispatch, anyhow::Error> {
	// Try to create the output file
	let file = fs::File::create("latest.log").context("Unable to create log file `latest.log`")?;

	// Create the dispatcher
	let dispatch = fern::Dispatch::new().format(self::fmt_log(false));
	let dispatch = self::set_libs_levels(dispatch, log::LevelFilter::Debug);

	// Note: On debug builds, log trace to file, else debug
	let dispatch = match cfg!(debug_assertions) {
		true => dispatch.level(log::LevelFilter::Trace),
		false => dispatch.level(log::LevelFilter::Debug),
	};

	Ok(dispatch.chain(file))
}

/// Sets the levels for a dispatch
fn set_libs_levels(dispatch: fern::Dispatch, max_level: log::LevelFilter) -> fern::Dispatch {
	// Note: By default in release builds we start at `Warn`
	let default_level = match cfg!(debug_assertions) {
		true => log::LevelFilter::Info,
		false => log::LevelFilter::Warn,
	};
	let default_level = default_level.min(max_level);

	// Note: Perf is debug on debug builds and off else
	let perf_level = match cfg!(debug_assertions) {
		true => log::LevelFilter::Debug,
		false => log::LevelFilter::Off,
	};
	let dispatch = dispatch.level_for("zsw::perf", perf_level.min(perf_level));

	// Filter out some modules to use the default level
	dispatch
		.level_for("wgpu", default_level)
		.level_for("wgpu_hal", default_level)
		.level_for("wgpu_core", default_level)
		.level_for("naga", default_level)
		.level_for("winit", default_level)
		.level_for("mio", default_level)
}

/// Returns a formatter for the logger
fn fmt_log(use_colors: bool) -> impl Fn(fern::FormatCallback, &fmt::Arguments, &log::Record) {
	move |out, msg, record: &log::Record| {
		let thread_name = DisplayWrapper::new(move |f| {
			let thread = thread::current();
			match thread.name() {
				Some(name) => write!(f, "{name}"),
				None => write!(f, "{}", thread.id().as_u64()),
			}
		});

		out.finish(format_args!(
			"{} [{}] ({thread_name}) {}: {msg}",
			chrono::Local::now().format(TIME_FMT),
			self::fmt_level(record.level(), use_colors),
			record.target(),
		));
	}
}

/// Formats a record
fn fmt_level(level: log::Level, use_colors: bool) -> impl fmt::Display {
	DisplayWrapper::new(move |f| match use_colors {
		true => write!(f, "{}", RECORD_COLORS.color(level)),
		false => write!(f, "{}", level),
	})
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

	let backtrace = std::backtrace::Backtrace::force_capture();

	log::error!("Thread '{thread_name}' panicked at '{msg}', {location}\nBacktrace:\n{backtrace}");
}
