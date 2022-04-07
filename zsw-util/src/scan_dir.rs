//! Directory scanning

// Imports
use std::{
	ops::{Generator, GeneratorState},
	path::PathBuf,
	pin::Pin,
};

/// Returns an iterator over all files in `path`.
///
/// # Errors
/// Ignores all errors reading directories / directory entries, simply logging them.
pub fn dir_files_iter(path: PathBuf) -> impl Iterator<Item = PathBuf> {
	// Get the generator
	let mut gen = self::dir_files_gen(path);

	// Then convert it to an iterator
	std::iter::from_fn(move || match gen.as_mut().resume(()) {
		GeneratorState::Yielded(path) => Some(path),
		GeneratorState::Complete(()) => None,
	})
}

/// Returns a generator that iterates over all files in `path` recursively
///
/// # Errors
/// Ignores all errors reading directories / directory entries, simply logging them.
// TODO: Not return a `Box<dyn Generator>`
pub fn dir_files_gen(path: PathBuf) -> Pin<Box<dyn Generator<Yield = PathBuf, Return = ()> + Send>> {
	#[allow(clippy::cognitive_complexity)] // It's fairly simple to understand
	let gen = move || {
		// Try to read the directory
		let dir = match std::fs::read_dir(&path) {
			Ok(dir) => dir,
			Err(err) => {
				tracing::warn!(?path, ?err, "Unable to read directory");
				return;
			},
		};

		// Then go through each entry
		for entry in dir {
			// Read the entry and file type
			let entry = match entry {
				Ok(entry) => entry,
				Err(err) => {
					tracing::warn!(?path, ?err, "Unable to read file entry");
					continue;
				},
			};
			let entry_path = entry.path();
			let file_type = match entry.file_type() {
				Ok(file_type) => file_type,
				Err(err) => {
					tracing::warn!(?entry_path, ?err, "Unable to read file type",);
					continue;
				},
			};

			match file_type.is_dir() {
				// Recurse on directories
				true => {
					let mut gen = self::dir_files_gen(entry_path);
					loop {
						match gen.as_mut().resume(()) {
							GeneratorState::Yielded(path) => yield path,
							GeneratorState::Complete(()) => (),
						}
					}
				},

				// Yield files
				false => yield entry_path,
			}
		}
	};

	Box::pin(gen)
}
