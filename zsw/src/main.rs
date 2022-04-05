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
	explicit_generic_args_with_impl_trait,
	async_closure,
	result_into_ok_or_err,
	generators,
	generator_trait,
	scoped_threads,
	derive_default_enum,
	mixed_integer_ops
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
	clippy::missing_const_for_fn,
	// This is a binary crate, so we don't expose any API
	rustdoc::private_intra_doc_links,
	// This is too prevalent on generic functions, which we don't want to ALWAYS be `Send`
	clippy::future_not_send,
)]

// Modules
mod app;
mod args;
mod logger;

// Exports
pub use self::args::Args;

// Imports
use {
	anyhow::Context,
	clap::StructOpt,
	std::{
		num::NonZeroUsize,
		sync::{
			atomic::{self, AtomicUsize},
			Arc,
		},
		thread,
	},
};

fn main() -> Result<(), anyhow::Error> {
	// Initialize logger
	match logger::init() {
		Ok(()) => log::debug!("Initialized logging"),
		Err(err) => eprintln!("Unable to initialize logger: {err:?}"),
	}

	// Initialize the tokio console subscriber if given the feature
	#[cfg(feature = "tokio-console")]
	console_subscriber::init();

	// Customize the rayon pool thread
	// Note: This is used indirectly in `image` by `jpeg-decoder`
	rayon::ThreadPoolBuilder::new()
		.thread_name(|idx| format!("rayon${idx}"))
		.build_global()
		.context("Unable to build `rayon` global thread pool")?;

	// Get arguments
	let args = match Args::try_parse() {
		Ok(args) => Arc::new(args),
		Err(err) => {
			log::warn!("Unable to retrieve arguments: {err:?}");
			err.exit();
		},
	};
	log::debug!("Arguments: {args:#?}");

	// Create the runtime and enter it
	let runtime = tokio::runtime::Builder::new_multi_thread()
	.worker_threads(2 * thread::available_parallelism().map_or(1, NonZeroUsize::get)) // TODO: Adjust?
	.enable_time()
    .thread_name_fn(|| {
       static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
       let id = NEXT_ID.fetch_add(1, atomic::Ordering::AcqRel);
       format!("tokio-runtime-{}", id)
    })
    .build()
	.context("Unable to create runtime")?;
	let _runtime_enter = runtime.enter();

	// Run the app and restart if we get an error (up to 5 errors)
	let mut errors = 0;
	while errors < 5 {
		match runtime.block_on(app::run(Arc::clone(&args))) {
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
