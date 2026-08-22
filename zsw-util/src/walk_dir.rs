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
	#[must_use]
	pub fn build(self, root: impl Into<PathBuf>) -> WalkDir {
		WalkDir {
			root:            root.into(),
			stack:           vec![],
			max_depth:       self.max_depth,
			recurse_symlink: self.recurse_symlink,
			is_finished:     false,
		}
	}
}

/// Directory walker
#[derive(Debug)]
pub struct WalkDir {
	/// Root
	root: PathBuf,

	/// Stack
	stack: Vec<fs::ReadDir>,

	/// Max depth
	max_depth: Option<usize>,

	/// Recurse on symlinks
	recurse_symlink: bool,

	/// Finished
	is_finished: bool,
}

impl WalkDir {
	/// Creates a new builder
	#[must_use]
	pub fn builder() -> WalkDirBuilder {
		WalkDirBuilder {
			max_depth:       None,
			recurse_symlink: false,
		}
	}
}

impl Iterator for WalkDir {
	type Item = Result<fs::DirEntry, io::Error>;

	fn next(&mut self) -> Option<Self::Item> {
		loop {
			// If we're finished, return `None`
			if self.is_finished {
				return None;
			}

			// Get the bottom-most directory, or create it from root.
			let cur_dir = match self.stack.last_mut() {
				Some(cur_dir) => cur_dir,
				_ => match fs::read_dir(self.root.clone()) {
					Ok(dir) => self.stack.push_mut(dir),
					Err(err) => return Some(Err(err)),
				},
			};

			// Then read the next entry
			let entry = match cur_dir.next() {
				Some(Ok(entry)) => entry,
				Some(Err(err)) => return Some(Err(err)),
				None => {
					// If we're done with self directory, pop it
					assert!(self.stack.pop().is_some(), "Stack should not be empty");

					// If we just popped the last directory, we're done
					self.is_finished |= self.stack.is_empty();

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
					Err(err) => return Some(Err(err)),
				};

				if is_maybe_dir {
					match fs::read_dir(entry.path()) {
						Ok(dir) => self.stack.push(dir),
						// Note: At this point we could have been following a symlink, so if it
						//       turns out it wasn't a directory, that's fine, we don't need to return an error.
						Err(err) =>
							if err.kind() != io::ErrorKind::NotADirectory {
								return Some(Err(err));
							},
					}
				}
			}

			return Some(Ok(entry));
		}
	}
}
