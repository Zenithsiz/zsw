//! Directory walker

use {
	futures::Stream,
	std::{fmt, future::Future, path::PathBuf, pin::Pin, task},
	tokio::{fs, io},
};

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
	/// A max depth of 0 means only the root directory is read.
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
	pub fn build(self, root: PathBuf) -> WalkDir {
		WalkDir {
			root,
			stack: vec![],
			max_depth: self.max_depth,
			recurse_symlink: self.recurse_symlink,
			is_finished: false,
			read_dir_fut: None,
			read_entry_metadata_fut: None,
		}
	}
}

/// Directory walker
#[pin_project::pin_project]
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

	/// Read directory future
	#[pin]
	read_dir_fut: Option<ReadDirFut>,

	/// Read entry metadata
	#[pin]
	read_entry_metadata_fut: Option<ReadMetadataFut>,
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

impl fmt::Debug for WalkDir {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		#[expect(clippy::ref_option, reason = "We don't want to use `.as_ref()` on the caller")]
		fn show_fut<F>(fut: &Option<F>) -> Option<&'static str> {
			fut.as_ref().map(|_| "...")
		}


		f.debug_struct("WalkDir")
			.field("root", &self.root)
			.field("stack", &self.stack)
			.field("max_depth", &self.max_depth)
			.field("recurse_symlink", &self.recurse_symlink)
			.field("is_finished", &self.is_finished)
			.field("read_dir_fut", &show_fut(&self.read_dir_fut))
			.field("read_entry_metadata_fut", &show_fut(&self.read_entry_metadata_fut))
			.finish()
	}
}

impl Stream for WalkDir {
	type Item = Result<fs::DirEntry, io::Error>;

	#[define_opaque(ReadDirFut, ReadMetadataFut)]
	fn poll_next(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Option<Self::Item>> {
		let mut this = self.as_mut().project();

		// If we're finished, return `None`
		if *this.is_finished {
			return task::Poll::Ready(None);
		}

		// If we're reading an entry's metadata, read it
		if let Some(read_entry_metadata_fut) = this.read_entry_metadata_fut.as_mut().as_pin_mut() {
			let res = task::ready!(read_entry_metadata_fut.poll(cx));
			this.read_entry_metadata_fut.set(None);
			let (entry_path, metadata) = res?;

			// If we found a directory, read it
			if metadata.is_dir() {
				let read_dir_fut = fs::read_dir(entry_path);
				this.read_dir_fut.set(Some(read_dir_fut));
			}
		}

		// If we're reading a directory, read it
		if let Some(read_dir_fut) = this.read_dir_fut.as_mut().as_pin_mut() {
			let res = task::ready!(read_dir_fut.poll(cx));
			this.read_dir_fut.set(None);
			let dir = res?;
			this.stack.push(dir);
		}

		// Get the bottom-most directory, or create it from root.
		let Some(cur_dir) = this.stack.last_mut() else {
			let read_dir_fut = fs::read_dir(this.root.clone());
			this.read_dir_fut.set(Some(read_dir_fut));
			return self.poll_next(cx);
		};

		// Then read the next entry
		let Some(entry) = task::ready!(cur_dir.poll_next_entry(cx)?) else {
			// If we're done with this directory, pop it
			assert!(this.stack.pop().is_some(), "Stack should not be empty");

			// If we just popped the last directory, we're done
			*this.is_finished |= this.stack.is_empty();

			return self.poll_next(cx);
		};

		// Read the entry metadata, so we know whether to recurse
		// Note: We also only care to read it if we have space to recurse
		if this.max_depth.is_none_or(|max_depth| this.stack.len() <= max_depth) {
			// Note: If we don't want to recurse on symlinks, we want to make sure we don't
			//       follow them, so we can detect them, and vice-versa
			let entry_path = entry.path();
			let follow_symlinks = *this.recurse_symlink;
			let read_entry_metadata_fut = async move {
				let metadata = match follow_symlinks {
					true => fs::metadata(&entry_path).await?,
					false => fs::symlink_metadata(&entry_path).await?,
				};

				Ok((entry_path, metadata))
			};
			this.read_entry_metadata_fut.set(Some(read_entry_metadata_fut));
		}

		task::Poll::Ready(Some(Ok(entry)))
	}
}

pub type ReadDirFut = impl Future<Output = Result<fs::ReadDir, io::Error>>;
#[expect(clippy::absolute_paths, reason = "We're already using `tokio::fs`")]
pub type ReadMetadataFut = impl Future<Output = Result<(PathBuf, std::fs::Metadata), io::Error>>;
