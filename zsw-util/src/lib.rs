//! Utility

#![feature(
	const_trait_impl,
	unboxed_closures,
	proc_macro_hygiene,
	stmt_expr_attributes,
	current_thread_id,
	oneshot_channel,
	extend_one
)]

pub mod loadable;
mod rect;
mod walk_dir;

pub use {
	self::{loadable::Loadable, rect::Rect, walk_dir::WalkDir},
	zsw_util_macros::*,
};

use {
	app_error::Context,
	core::{cmp, str::FromStr},
	serde::de::DeserializeOwned,
	std::{ffi::OsStr, fs, path::Path, thread},
	zutil_cloned::cloned,
};

/// App error export with our data
pub type AppError = app_error::AppError<()>;

/// Returns the maximum value in an array as a `const fn`
#[must_use]
pub const fn array_max<const N: usize>(values: &[usize; N]) -> Option<usize> {
	let mut max = None;
	let mut cur_idx = 0;
	while cur_idx < values.len() {
		let value = values[cur_idx];

		max = Some(match max {
			Some(max) => self::usize_max(max, value),
			None => value,
		});

		cur_idx += 1;
	}

	max
}

/// Returns the maximum between two `usize` values
const fn usize_max(lhs: usize, rhs: usize) -> usize {
	if lhs > rhs { lhs } else { rhs }
}

/// Spawns a task
#[track_caller]
pub fn spawn_task<F>(name: impl Into<String>, f: F)
where
	F: FnOnce() -> Result<(), AppError> + Send + 'static,
{
	let name = name.into();

	#[cloned(name)]
	let f = move || {
		let id = thread::current_id();
		tracing::debug!("Spawning task {name:?} ({id:?})");
		match f() {
			Ok(()) => tracing::debug!("Task {name:?} ({id:?}) finished"),
			Err(err) => tracing::warn!("Task {name:?} ({id:?}) returned error: {err:?}"),
		}
	};

	if let Err(err) = thread::Builder::new().name(name.clone()).spawn(f) {
		let err = AppError::new(&err);
		tracing::warn!("Unable to spawn task {name:?}: {err:?}");
	}
}

/// Reads all toml files in a directory as values.
///
/// The key will be their name, excluding the `.toml` extension.
pub fn read_dir_all_toml<K, V, R>(dir: &Path) -> Result<R, AppError>
where
	K: FromStr + Send + 'static,
	V: DeserializeOwned + Send + Sync + 'static,
	R: Default + Extend<(K, V)>,
	// TODO: This bound is ugly, can we make it better?
	Result<K, K::Err>: Context<(), Output = Result<K, AppError>>,
{
	fs::create_dir_all(dir).context("Unable to create root directory")?;
	let dir = fs::read_dir(dir).context("Unable to read directory")?;

	let mut values = R::default();
	for entry in dir {
		// Ignore directories and non `.toml` files
		let entry = entry.context("Unable to get entry")?;
		let entry_path = entry.path();
		if entry.file_type().context("Unable to get entry metadata")?.is_dir() ||
			entry_path.extension().and_then(OsStr::to_str) != Some("toml")
		{
			continue;
		}

		// Then get the name from the file
		let name = entry_path.file_stem().context("Entry path had no file stem")?;
		let name = name
			.to_str()
			.with_context(|| format!("Entry name was non-utf8: {name:?}"))?;
		let name = name
			.parse::<K>()
			.with_context(|| format!("Entry name was invalid {name:?}"))?;

		// Try to read the file
		let toml = fs::read_to_string(&entry_path).with_context(|| format!("Unable to read file {entry_path:?}"))?;

		// And parse it
		let value = toml::from_str(&toml).with_context(|| format!("Unable to parse file {entry_path:?}"))?;

		values.extend_one((name, value));
	}

	Ok(values)
}

/// Compares `value` to the interval `lhs..rhs`
#[must_use]
pub fn cmp_interval(value: f32, lhs: f32, rhs: f32) -> cmp::Ordering {
	if value < lhs {
		return cmp::Ordering::Less;
	}
	if value > rhs {
		return cmp::Ordering::Greater;
	}

	cmp::Ordering::Equal
}
