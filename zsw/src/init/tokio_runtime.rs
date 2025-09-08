//! Tokio initialization

// Imports
use {
	app_error::Context,
	std::{
		num::NonZeroUsize,
		sync::atomic::{self, AtomicUsize},
		thread,
	},
	zsw_util::AppError,
};

/// Creates the tokio runtime
pub fn create(worker_threads: Option<NonZeroUsize>) -> Result<tokio::runtime::Runtime, AppError> {
	let worker_threads = match worker_threads {
		Some(worker_threads) => worker_threads.get(),
		None => thread::available_parallelism().map(NonZeroUsize::get).unwrap_or(1),
	};

	tokio::runtime::Builder::new_multi_thread()
		.enable_time()
		.enable_io()
		.thread_name_fn(|| {
			static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
			let id = NEXT_ID.fetch_add(1, atomic::Ordering::AcqRel);
			format!("tokio${id}")
		})
		.worker_threads(worker_threads)
		.build()
		.context("Unable to create runtime")
}
