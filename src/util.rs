//! Utility

// Modules
mod scan_dir;

// Exports
pub use scan_dir::visit_files_dir;

// Imports
use std::{
	hash::{Hash, Hasher},
	time::{Duration, Instant},
};

/// Measures how long it took to execute a function
pub fn measure<T>(f: impl FnOnce() -> T) -> (T, Duration) {
	let start_time = Instant::now();
	let value = f();
	let duration = Instant::now().saturating_duration_since(start_time);
	(value, duration)
}

pub macro measure_dbg {
	() => {
		::std::eprintln!("[{}:{}]", ::std::file!(), ::std::line!())
	},
	($value:expr $(,)?) => {
		match $crate::util::measure(move || $value) {
			(value, duration) => {
				::std::eprintln!("[{}:{}] {} took {:?}",
					::std::file!(), ::std::line!(), ::std::stringify!($value), duration);
				value
			}
		}
	},
	($($val:expr),+ $(,)?) => {
		($(::std::dbg!($val)),+,)
	}
}

/// Hashes a value using `twox_hash`
pub fn _hash_of<T: ?Sized + Hash>(value: &T) -> u64 {
	let mut hasher = twox_hash::XxHash64::with_seed(0);
	value.hash(&mut hasher);
	hasher.finish()
}
