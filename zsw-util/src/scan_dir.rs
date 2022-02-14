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
	let gen = self::dir_files_gen(path);

	// Then convert the it to an iterator
	let mut gen = Pin::from(gen);
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
pub fn dir_files_gen(path: PathBuf) -> Box<dyn Generator<Yield = PathBuf, Return = ()> + Send> {
	let gen = move || {
		// Try to read the directory
		let dir = match std::fs::read_dir(&path) {
			Ok(dir) => dir,
			Err(err) => {
				log::warn!("Unable to read directory `{path:?}`: {:?}", anyhow::anyhow!(err));
				return;
			},
		};

		// Then go through each entry
		for entry in dir {
			// Read the entry and file type
			let entry = match entry {
				Ok(entry) => entry,
				Err(err) => {
					log::warn!("Unable to read file entry in `{path:?}`: {:?}", anyhow::anyhow!(err));
					continue;
				},
			};
			let entry_path = entry.path();
			let file_type = match entry.file_type() {
				Ok(file_type) => file_type,
				Err(err) => {
					log::warn!(
						"Unable to read file type for `{entry_path:?}`: {:?}",
						anyhow::anyhow!(err)
					);
					continue;
				},
			};

			match file_type.is_dir() {
				// Recurse on directories
				true => {
					let mut gen = Pin::from(self::dir_files_gen(entry_path));
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

	Box::new(gen)
}
