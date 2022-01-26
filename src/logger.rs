//! Logger

// Imports
use crate::util::DisplayWrapper;
use anyhow::Context;
use fern::colors::{Color, ColoredLevelConfig};
use std::{fmt, fs, panic, thread};

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
	// Create the base dispatcher
	let mut dispatcher = fern::Dispatch::new().chain(self::stderr_dispatch());

	// Try to add a file dispatch
	let file_err = match self::file_dispatch() {
		Ok(file) => {
			dispatcher = dispatcher.chain(file);
			None
		},
		Err(err) => Some(err),
	};

	// Then apply
	dispatcher.apply().context("Unable to initialize logger")?;

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
	fern::Dispatch::new()
		.format(self::fmt_log(true))
		.level_for("wgpu", log::LevelFilter::Warn)
		.level_for("wgpu_hal", log::LevelFilter::Warn)
		.level_for("wgpu_core", log::LevelFilter::Warn)
		.level_for("naga", log::LevelFilter::Warn)
		.level_for("winit", log::LevelFilter::Warn)
		.level(log::LevelFilter::Info)
		.chain(std::io::stderr())
}

/// Returns the file dispatch
fn file_dispatch() -> Result<fern::Dispatch, anyhow::Error> {
	// Try to create the output file
	let file = fs::File::create("latest.log").context("Unable to create log file `latest.log`")?;

	let dispatcher = fern::Dispatch::new()
		.format(self::fmt_log(false))
		.level_for("wgpu", log::LevelFilter::Warn)
		.level_for("wgpu_hal", log::LevelFilter::Warn)
		.level_for("wgpu_core", log::LevelFilter::Warn)
		.level_for("naga", log::LevelFilter::Warn)
		.level_for("winit", log::LevelFilter::Warn);

	// On debug builds, log trace to file, else debug
	#[cfg(debug_assertions)]
	let dispatcher = dispatcher.level(log::LevelFilter::Trace);

	#[cfg(not(debug_assertions))]
	let dispatcher = dispatcher.level(log::LevelFilter::Debug);

	Ok(dispatcher.chain(file))
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
pub fn fmt_level(level: log::Level, use_colors: bool) -> impl fmt::Display {
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
