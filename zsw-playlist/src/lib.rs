//! Image playlist
//!
//! Manages the paths/urls of all images to display.

// Features
#![feature(never_type)]
// Lints
#![warn(
	clippy::pedantic,
	clippy::nursery,
	missing_copy_implementations,
	missing_debug_implementations,
	noop_method_call,
	unused_results
)]
#![deny(
	// We want to annotate unsafe inside unsafe fns
	unsafe_op_in_unsafe_fn,
	// We muse use `expect` instead
	clippy::unwrap_used
)]
#![allow(
	// Style
	clippy::implicit_return,
	clippy::multiple_inherent_impl,
	clippy::pattern_type_mismatch,
	// `match` reads easier than `if / else`
	clippy::match_bool,
	clippy::single_match_else,
	//clippy::single_match,
	clippy::self_named_module_files,
	clippy::items_after_statements,
	clippy::module_name_repetitions,
	// Performance
	clippy::suboptimal_flops, // We prefer readability
	// Some functions might return an error in the future
	clippy::unnecessary_wraps,
	// Due to working with windows and rendering, which use `u32` / `f32` liberally
	// and interchangeably, we can't do much aside from casting and accepting possible
	// losses, although most will be lossless, since we deal with window sizes and the
	// such, which will fit within a `f32` losslessly.
	clippy::cast_precision_loss,
	clippy::cast_possible_truncation,
	// We use proper error types when it matters what errors can be returned, else,
	// such as when using `anyhow`, we just assume the caller won't check *what* error
	// happened and instead just bubbles it up
	clippy::missing_errors_doc,
	// Too many false positives and not too important
	clippy::missing_const_for_fn,
	// This is a binary crate, so we don't expose any API
	rustdoc::private_intra_doc_links,
)]

// Imports
use {
	async_lock::{Mutex, MutexGuard},
	rand::prelude::SliceRandom,
	std::{collections::HashSet, path::PathBuf, sync::Arc},
	zsw_side_effect_macros::side_effect,
	zsw_util::{extse::AsyncLockMutexSe, MightBlock, MightLock},
};

/// Inner
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct Inner {
	/// Root path
	// TODO: Use this properly
	root_path: Option<PathBuf>,

	/// All images
	images: HashSet<Arc<PlaylistImage>>,

	/// Current images
	cur_images: Vec<Arc<PlaylistImage>>,
}

/// Image playlist
#[derive(Debug)]
pub struct Playlist {
	/// Image sender
	img_tx: async_channel::Sender<Arc<PlaylistImage>>,

	/// Image receiver
	img_rx: async_channel::Receiver<Arc<PlaylistImage>>,

	/// Inner
	inner: Mutex<Inner>,

	/// Lock source
	lock_source: LockSource,
}

impl Playlist {
	/// Creates a new, empty, playlist
	#[must_use]
	pub fn new() -> Self {
		// Note: Making the close channel unbounded is what allows us to not block
		//       in `Self::stop`.
		let (img_tx, img_rx) = async_channel::bounded(1);

		// Create the empty inner data
		let inner = Inner {
			root_path:  None,
			images:     HashSet::new(),
			cur_images: vec![],
		};

		Self {
			img_tx,
			img_rx,
			inner: Mutex::new(inner),
			lock_source: LockSource,
		}
	}

	/// Creates an inner lock
	///
	/// # Blocking
	/// Will block until any existing inner locks are dropped
	#[side_effect(MightLock<PlaylistLock<'_>>)]
	pub async fn lock_inner(&self) -> PlaylistLock<'_> {
		// DEADLOCK: Caller is responsible to ensure we don't deadlock
		//           We don't lock it outside of this method
		let guard = self.inner.lock_se().await.allow::<MightBlock>();
		PlaylistLock::new(guard, &self.lock_source)
	}

	/// Runs the playlist
	///
	/// # Locking
	/// Locks the `PlaylistLock` lock on `self`
	#[side_effect(MightLock<PlaylistLock<'_>>)]
	pub async fn run(&self) -> ! {
		loop {
			// Get the next image to send
			// DEADLOCK: Caller ensures we can lock it
			// Note: It's important to not have this in the match expression, as it would
			//       keep the lock through the whole match.
			let next = self
				.lock_inner()
				.await
				.allow::<MightLock<PlaylistLock>>()
				.get_mut(&self.lock_source)
				.cur_images
				.pop();


			// Then check if we got it
			match next {
				// If we got it, send it
				// DEADLOCK: We don't hold any locks while sending
				// Note: This can't return an `Err` because `self` owns a receiver
				Some(image) => self.img_tx.send(image).await.expect("Image receiver was closed"),

				// Else get the next batch and shuffle them
				// DEADLOCK: Caller ensures we can lock it.
				None => {
					let mut inner = self.lock_inner().await.allow::<MightLock<PlaylistLock>>();
					let inner = inner.get_mut(&self.lock_source);
					inner.cur_images.extend(inner.images.iter().cloned());
					inner.cur_images.shuffle(&mut rand::thread_rng());
				},
			}
		}
	}

	/// Removes an image
	pub async fn remove_image<'a>(&'a self, playlist_lock: &mut PlaylistLock<'a>, image: &PlaylistImage) {
		// Note: We don't care if the image actually existed or not
		let _ = playlist_lock.get_mut(&self.lock_source).images.remove(image);
	}

	/// Sets the root path
	pub async fn set_root_path<'a>(&'a self, playlist_lock: &mut PlaylistLock<'a>, root_path: PathBuf) {
		let inner = playlist_lock.get_mut(&self.lock_source);

		// Remove all existing paths and add new ones
		inner.images.clear();
		for path in zsw_util::dir_files_iter(root_path.clone()) {
			let _ = inner.images.insert(Arc::new(PlaylistImage::File(path)));
		}

		// Remove all current paths too
		inner.cur_images.clear();

		// Save the root path
		inner.root_path = Some(root_path);
	}

	/// Returns the root path
	pub async fn root_path<'a>(&'a self, playlist_lock: &PlaylistLock<'a>) -> Option<PathBuf> {
		playlist_lock.get(&self.lock_source).root_path.clone()
	}

	/// Retrieves the next image
	///
	/// # Locking
	/// Locks the `PlaylistLock` lock on `self`
	// Note: Doesn't literally lock it, but the other side of the channel
	//       needs to lock it in order to progress, so it's equivalent
	#[side_effect(MightLock<PlaylistLock<'_>>)]
	pub async fn next(&self) -> Arc<PlaylistImage> {
		// Note: This can't return an `Err` because `self` owns a sender
		// DEADLOCK: Caller ensures it won't hold an `PlaylistLock`,
		//           and we ensure the other side of the channel
		//           can progress.
		self.img_rx.recv().await.expect("Image sender was closed")
	}

	/// Peeks the next images
	pub async fn peek_next(&self, playlist_lock: &PlaylistLock<'_>, mut f: impl FnMut(&PlaylistImage) + Send) {
		let inner = playlist_lock.get(&self.lock_source);

		for image in inner.cur_images.iter().rev() {
			f(image);
		}
	}
}

impl Default for Playlist {
	fn default() -> Self {
		Self::new()
	}
}

/// Source for all locks
// Note: This is to ensure user can't create the locks themselves
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct LockSource;

/// Inner lock
// TODO: Rename to `PlaylistLock`, maybe?
pub type PlaylistLock<'a> = zsw_util::Lock<'a, MutexGuard<'a, Inner>, LockSource>;


/// A playlist image
#[derive(PartialEq, Eq, Clone, Hash, Debug)]
pub enum PlaylistImage {
	/// File path
	File(PathBuf),
	// TODO: URL
}
