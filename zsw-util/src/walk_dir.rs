//! Directory walker

use std::{fs, io, path::PathBuf};

/// Directory walker builder
#[derive(Debug)]
#[expect(missing_copy_implementations, reason = "We might have non-Copy fields in the future")]
pub struct WalkDirBuilder {
	/// Max depth
	max_depth: Option<usize>,

	/// Recurse on symlinks
	recurse_symlink: bool,
}

impl WalkDirBuilder {
	/// Sets the max depth for walking.
	///
	/// A max depth of 0 means only the root directory entries
	/// are read.
	#[must_use]
	pub fn max_depth(self, max_depth: Option<usize>) -> Self {
		Self { max_depth, ..self }
	}

	/// Sets if we should recurse on symlinks
	#[must_use]
	pub fn recurse_symlink(self, recurse_symlink: bool) -> Self {
		Self {
			recurse_symlink,
			..self
		}
	}

	/// Builders the directory walker
	///
	/// This will read the root directory so you can
	/// catch errors earlier
	pub fn build(self, root_path: impl Into<PathBuf>) -> Result<WalkDir, io::Error> {
		let root_path = root_path.into();
		let root = fs::read_dir(&root_path)?;
		Ok(WalkDir {
			stack:           vec![(root_path, root)],
			max_depth:       self.max_depth,
			recurse_symlink: self.recurse_symlink,
		})
	}
}

/// Directory walker
#[derive(Debug)]
pub struct WalkDir {
	/// Stack
	stack: Vec<(PathBuf, fs::ReadDir)>,

	/// Max depth
	max_depth: Option<usize>,

	/// Recurse on symlinks
	recurse_symlink: bool,
}

impl WalkDir {
	/// Creates a new builder.
	#[must_use]
	pub fn builder() -> WalkDirBuilder {
		WalkDirBuilder {
			max_depth:       None,
			recurse_symlink: false,
		}
	}
}

impl Iterator for WalkDir {
	type Item = Result<fs::DirEntry, WalkDirError>;

	fn next(&mut self) -> Option<Self::Item> {
		loop {
			// Get the bottom-most directory.
			// Note: If we've already popped it, then we're done
			let (cur_dir_path, cur_dir) = self.stack.last_mut()?;

			// Then read the next entry
			let entry = match cur_dir.next() {
				Some(Ok(entry)) => entry,
				Some(Err(err)) =>
					return Some(Err(WalkDirError::ReadDirEntry {
						dir_path: cur_dir_path.clone(),
						err,
					})),
				None => {
					// If we're done with self directory, pop it
					assert!(self.stack.pop().is_some(), "Stack should not be empty");
					continue;
				},
			};

			// Read the entry metadata, so we know whether to recurse
			// Note: We also only care to read it if we have space to recurse
			if self.max_depth.is_none_or(|max_depth| self.stack.len() < max_depth) {
				let is_maybe_dir = match entry.file_type() {
					Ok(file_type) => match file_type.is_dir() {
						true => true,
						false => self.recurse_symlink && file_type.is_symlink(),
					},
					Err(err) =>
						return Some(Err(WalkDirError::FileTypeEntry {
							path: entry.path(),
							err,
						})),
				};

				if is_maybe_dir {
					let path = entry.path();
					match fs::read_dir(&path) {
						Ok(dir) => self.stack.push((path, dir)),
						// Note: At this point we could have been following a symlink, so if it
						//       turns out it wasn't a directory, that's fine, we don't need to return an error.
						Err(err) =>
							if err.kind() != io::ErrorKind::NotADirectory {
								return Some(Err(WalkDirError::ReadDir { path, err }));
							},
					}
				}
			}

			return Some(Ok(entry));
		}
	}
}

/// Error for [`WalkDir`]
#[derive(Debug, thiserror::Error)]
pub enum WalkDirError {
	#[error("Unable to read directory entry in {}", dir_path.display())]
	ReadDirEntry {
		dir_path: PathBuf,

		#[source]
		err: io::Error,
	},

	#[error("Unable to get file type of {}", path.display())]
	FileTypeEntry {
		path: PathBuf,

		#[source]
		err: io::Error,
	},

	#[error("Unable to read directory {}", path.display())]
	ReadDir {
		path: PathBuf,

		#[source]
		err: io::Error,
	},
}
