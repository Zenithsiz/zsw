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
	try_trait_v2,
	backtrace,
	thread_id_value
)]
// Lints
#![warn(
	clippy::pedantic,
	clippy::nursery,
	missing_copy_implementations,
	missing_debug_implementations,
	noop_method_call,
	unused_results
)]
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
mod logger;
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

fn main() -> Result<(), anyhow::Error> {
	// Initialize logger
	match logger::init() {
		Ok(()) => log::debug!("Initialized logging"),
		Err(err) => eprintln!("Unable to initialize logger: {err:?}"),
	}

	// Get arguments
	let args = args::get().context("Unable to retrieve arguments")?;
	log::debug!("Arguments: {args:#?}");

	// Create the app
	let app = App::new(args).block_on().context("Unable to initialize app")?;

	// Then run it
	app.run().context("Unable to run app")?;

	// TODO: Wgpu seems to segfault during a global destructor.
	//       Not sure what's causing this, but happens after "Destroying 2 command encoders"
	//       log comes out. If instead of 2, 3 are being destroyed, it doesn't segfault.

	Ok(())
}
