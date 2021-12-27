//! Utility

// Modules
mod scan_dir;

// Exports
pub use scan_dir::visit_files_dir;

// Imports
use std::time::{Duration, Instant};

/// Measures how long it took to execute a function
pub fn measure<T>(f: impl FnOnce() -> T) -> (T, Duration) {
	let start_time = Instant::now();
	let value = f();
	let duration = Instant::now().saturating_duration_since(start_time);
	(value, duration)
}
