//! Rayon initialization
//!
//! Some dependencies use rayon, so we customize it's thread pool.

// Imports
use {
	crate::AppError,
	anyhow::Context,
	std::{num::NonZeroUsize, thread},
};

pub fn init(worker_threads: Option<NonZeroUsize>) -> Result<(), AppError> {
	let worker_threads = match worker_threads {
		Some(worker_threads) => worker_threads.get(),
		None => thread::available_parallelism().map(NonZeroUsize::get).unwrap_or(1),
	};

	rayon::ThreadPoolBuilder::new()
		.thread_name(|idx| format!("rayon${idx}"))
		.num_threads(worker_threads)
		.build_global()
		.context("Unable to build `rayon` global thread pool")?;

	Ok(())
}
