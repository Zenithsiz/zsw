//! Directory scanning

// Imports
use std::{
	ops::ControlFlow,
	path::{Path, PathBuf},
};

/// Visits all files in `path`, recursively.
///
/// # Errors
/// Ignores all errors reading directories, simply logging them.
///
/// # Return
/// Returns the number of files successfully loaded
pub fn visit_files_dir<E, F>(path: &Path, f: &mut F) -> Result<usize, E>
where
	F: FnMut(PathBuf) -> ControlFlow<E>,
{
	let mut files_loaded = 0;
	let dir = match std::fs::read_dir(path) {
		Ok(dir) => dir,
		Err(err) => {
			log::warn!("Unable to read directory `{path:?}`: {:?}", anyhow::anyhow!(err));
			return Ok(0);
		},
	};
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
				files_loaded += self::visit_files_dir(&entry.path(), f)?;
			},

			// Visit files
			false => match f(entry_path) {
				ControlFlow::Continue(()) => files_loaded += 1,
				ControlFlow::Break(err) => return Err(err),
			},
		}
	}

	Ok(files_loaded)
}
