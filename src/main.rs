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
#![warn(clippy::pedantic, clippy::nursery)]
#![deny(
	// We want to annotate unsafe inside unsafe fns
	unsafe_op_in_unsafe_fn,
	// We muse use `expect` instead
	clippy::unwrap_used
)]
#![allow(
	// Style
	clippy::implicit_return,
	clippy::multiple_inherent_impl,
	clippy::pattern_type_mismatch,
	// `match` reads easier than `if / else`
	clippy::match_bool,
	clippy::single_match_else,
	//clippy::single_match,
	clippy::self_named_module_files,
	clippy::items_after_statements,
	clippy::module_name_repetitions,
	// Performance
	clippy::suboptimal_flops, // We prefer readability
	// Some functions might return an error in the future
	clippy::unnecessary_wraps,
	// Due to working with windows and rendering, which use `u32` / `f32` liberally
	// and interchangeably, we can't do much aside from casting and accepting possible
	// losses, although most will be lossless, since we deal with window sizes and the
	// such, which will fit within a `f32` losslessly.
	clippy::cast_precision_loss,
	// We use proper error types when it matters what errors can be returned, else,
	// such as when using `anyhow`, we just assume the caller won't check *what* error
	// happened and instead just bubbles it up
	clippy::missing_errors_doc,
)]

// Modules
mod app;
mod args;
mod egui;
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
	egui::Egui,
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
