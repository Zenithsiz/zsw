//! Directory scanning

// Imports
use std::{
	ops::Try,
	path::{Path, PathBuf},
};

/// Visits all files in `path`, recursively.
///
/// # Errors
/// Ignores all errors reading directories, simply logging them.
pub fn visit_files_dir<T: Try<Output = ()>, F>(path: &Path, f: &mut F) -> T
where
	F: FnMut(PathBuf) -> T,
{
	// Try to read the directory
	let dir = match std::fs::read_dir(path) {
		Ok(dir) => dir,
		Err(err) => {
			log::warn!("Unable to read directory `{path:?}`: {:?}", anyhow::anyhow!(err));
			return T::from_output(());
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
			true => self::visit_files_dir(&entry.path(), f)?,

			// Visit files
			false => f(entry_path)?,
		}
	}

	T::from_output(())
}
