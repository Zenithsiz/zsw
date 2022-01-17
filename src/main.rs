//! Zenithsiz's scrolling wallpaper

// Features
#![feature(
	never_type,
	available_parallelism,
	control_flow_enum,
	decl_macro,
	inline_const,
	destructuring_assignment,
	stmt_expr_attributes,
	try_trait_v2
)]
// Lints
#![deny(unsafe_op_in_unsafe_fn)]

// Modules
mod app;
mod args;
mod img;
mod panel;
mod path_loader;
mod rect;
mod sync;
mod util;
mod wgpu;

// Exports
pub use self::{
	app::App,
	args::Args,
	img::ImageLoader,
	panel::{Panel, PanelState, PanelsRenderer},
	path_loader::PathLoader,
	rect::Rect,
	wgpu::Wgpu,
};

// Imports
use anyhow::Context;
use pollster::FutureExt;
use std::fs;

fn main() -> Result<(), anyhow::Error> {
	// Initialize logger
	match self::init_log() {
		Ok(()) => log::debug!("Initialized logging"),
		Err(err) => eprintln!("Unable to initialize logger: {err:?}"),
	}

	// Get arguments
	let args = args::get().context("Unable to retrieve arguments")?;
	log::debug!("Found arguments {args:?}");

	// Create the app
	let app = App::new(args).block_on().context("Unable to initialize app")?;

	// Then run it
	app.run().context("Unable to run app")?;

	Ok(())
}

/// Initializes the logging
fn init_log() -> Result<(), anyhow::Error> {
	/// Creates the file logger
	// TODO: Put back to trace once wgpu is somewhat filtered out
	fn file_logger() -> Result<Box<simplelog::WriteLogger<fs::File>>, anyhow::Error> {
		let file = fs::File::create("latest.log").context("Unable to create file `latest.log`")?;
		Ok(simplelog::WriteLogger::new(
			log::LevelFilter::Info,
			simplelog::Config::default(),
			file,
		))
	}

	// All loggers
	let mut loggers = Vec::with_capacity(2);

	// Create the term logger
	let term_logger = simplelog::TermLogger::new(
		log::LevelFilter::Info,
		simplelog::Config::default(),
		simplelog::TerminalMode::Stderr,
		simplelog::ColorChoice::Auto,
	);
	loggers.push(term_logger as Box<_>);

	// Then try to create the file logger
	let file_logger_res = file_logger().map(|file_logger| loggers.push(file_logger as _));

	// Finally initialize them all
	simplelog::CombinedLogger::init(loggers).context("Unable to initialize loggers")?;

	// Then check if we got any errors
	if let Err(err) = file_logger_res {
		log::warn!("Unable to initialize file logger: {err:?}");
	}

	Ok(())
}
