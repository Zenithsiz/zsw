//! Zenithsiz's scrolling wallpaper

// Features
#![feature(
	never_type,
	control_flow_enum,
	decl_macro,
	inline_const,
	stmt_expr_attributes,
	try_trait_v2,
	backtrace,
	thread_id_value,
	unwrap_infallible,
	explicit_generic_args_with_impl_trait
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
	clippy::cast_possible_truncation,
	// We use proper error types when it matters what errors can be returned, else,
	// such as when using `anyhow`, we just assume the caller won't check *what* error
	// happened and instead just bubbles it up
	clippy::missing_errors_doc,
	// Too many false positives and not too important
	clippy::missing_const_for_fn
)]

// Modules
mod app;
mod args;
mod egui;
mod img;
mod logger;
mod panel;
mod paths;
mod rect;
mod util;
mod wgpu;

// Exports
pub use self::{
	args::Args,
	egui::Egui,
	img::ImageLoader,
	panel::{Panel, PanelState, Panels, PanelsProfile, PanelsRenderer},
	rect::Rect,
	wgpu::Wgpu,
};

// Imports
use anyhow::Context;

fn main() -> Result<(), anyhow::Error> {
	// Initialize logger
	match logger::init() {
		Ok(()) => log::debug!("Initialized logging"),
		Err(err) => eprintln!("Unable to initialize logger: {err:?}"),
	}

	// Initialize the deadlock detection
	#[cfg(debug_assertions)]
	self::deadlock_init();

	// Get arguments
	let args = args::get().context("Unable to retrieve arguments")?;
	log::debug!("Arguments: {args:#?}");

	// Run the app and restart if we get an error (up to 5 errors)
	let mut errors = 0;
	while errors < 5 {
		match app::run(&args) {
			Ok(()) => {
				log::info!("Application finished");
				break;
			},
			Err(err) => {
				log::error!("Application encountered fatal error: {err:?}");
				errors += 1;
				continue;
			},
		}
	}

	Ok(())
}

/// Initializes deadlock detection
#[cfg(debug_assertions)]
fn deadlock_init() {
	// Create a background thread which checks for deadlocks every 10s
	#[allow(clippy::let_underscore_drop)] // We want to detach the thread
	let _ = std::thread::spawn(move || loop {
		// Sleep so we aren't continuously checking
		std::thread::sleep(std::time::Duration::from_secs(10));

		// Then check if we have any and continue if we don't
		log::debug!("Checking for deadlocks");
		let deadlocks = parking_lot::deadlock::check_deadlock();
		if deadlocks.is_empty() {
			log::debug!("Found no deadlocks");
			continue;
		}

		// If we do, log them
		log::warn!("Detected {} deadlocks", deadlocks.len());
		for (idx, threads) in deadlocks.iter().enumerate() {
			log::warn!("Deadlock #{idx}");
			for thread in threads {
				log::warn!("\tThread Id {:#?}", thread.thread_id());
				log::warn!("\tBacktrace: {:#?}", thread.backtrace());
			}
		}
	});
}
