//! Utility

// Modules
mod display_wrapper;
mod scan_dir;

// Exports
pub use display_wrapper::DisplayWrapper;
pub use scan_dir::visit_files_dir;

// Imports
use anyhow::Context;
use std::{
	fs,
	hash::{Hash, Hasher},
	path::Path,
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
		match $crate::util::measure(|| $value) {
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

/// Spawns a new thread using `crossbeam::thread::Scope` with name
pub fn spawn_scoped<'scope, 'env, T, F>(
	s: &'scope crossbeam::thread::Scope<'env>, name: impl Into<String>, f: F,
) -> Result<crossbeam::thread::ScopedJoinHandle<'scope, T>, anyhow::Error>
where
	T: Send + 'env,
	F: Send + FnOnce() -> T + 'env,
{
	let name = name.into();
	s.builder()
		.name(name.clone())
		.spawn(|_| f())
		.with_context(|| format!("Unable to start thread {name:?}"))
}

/// Spawns multiple scoped threads
pub fn spawn_scoped_multiple<'scope, 'env, T, F>(
	s: &'scope crossbeam::thread::Scope<'env>, name: impl Into<String>, threads: usize, mut f: impl FnMut() -> F,
) -> Result<Vec<crossbeam::thread::ScopedJoinHandle<'scope, T>>, anyhow::Error>
where
	T: Send + 'env,
	F: Send + FnOnce() -> T + 'env,
{
	let name = name.into();
	(0..threads).map(move |_| self::spawn_scoped(s, &name, f())).collect()
}

/// Parses json from a file
pub fn parse_json_from_file<T: serde::de::DeserializeOwned>(path: impl AsRef<Path>) -> Result<T, anyhow::Error> {
	// Open the file
	let file = fs::File::open(path).context("Unable to open file")?;

	// Then parse it
	serde_json::from_reader(file).context("Unable to parse file")
}

/// Hashes a value using `twox_hash`
pub fn _hash_of<T: ?Sized + Hash>(value: &T) -> u64 {
	let mut hasher = twox_hash::XxHash64::with_seed(0);
	value.hash(&mut hasher);
	hasher.finish()
}
