//! Zenithsiz's scrolling wallpaper

// Features
#![feature(
	never_type,
	control_flow_enum,
	decl_macro,
	inline_const,
	stmt_expr_attributes,
	try_trait_v2,
	thread_id_value,
	unwrap_infallible,
	async_closure,
	generators,
	generator_trait,
	associated_type_bounds
)]

// Modules
mod app;
mod args;
mod trace;

// Exports
pub use self::args::Args;

// Imports
use {
	anyhow::Context,
	clap::Parser,
	std::sync::atomic::{self, AtomicUsize},
};

fn main() -> Result<(), anyhow::Error> {
	// Initialize tracing
	trace::init();

	// Customize the rayon pool thread
	// Note: This is used indirectly in `image` by `jpeg-decoder`
	rayon::ThreadPoolBuilder::new()
		.thread_name(|idx| format!("rayon${idx}"))
		.build_global()
		.context("Unable to build `rayon` global thread pool")?;

	// Get arguments
	let args = match Args::try_parse() {
		Ok(args) => args,
		Err(err) => {
			tracing::warn!(?err, "Unable to retrieve arguments");
			err.exit();
		},
	};
	tracing::debug!(?args, "Arguments");

	// Create the runtime and enter it
	let runtime = self::create_runtime()?;
	let _runtime_enter = runtime.enter();

	// Run the app and restart if we get an error (up to 5 errors)
	let mut errors = 0;
	while errors < 5 {
		match runtime.block_on(app::run(&args)) {
			Ok(()) => {
				tracing::info!("Application finished");
				break;
			},
			Err(err) => {
				tracing::error!(?err, "Application encountered fatal error");
				errors += 1;
				continue;
			},
		}
	}

	Ok(())
}

/// Creates the tokio runtime
fn create_runtime() -> Result<tokio::runtime::Runtime, anyhow::Error> {
	tokio::runtime::Builder::new_multi_thread()
		.enable_time()
		.thread_name_fn(|| {
			static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
			let id = NEXT_ID.fetch_add(1, atomic::Ordering::AcqRel);
			format!("tokio-runtime-{id}")
		})
		.build()
		.context("Unable to create runtime")
}
