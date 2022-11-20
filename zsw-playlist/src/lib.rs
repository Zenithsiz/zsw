//! Image playlist
//!
//! Manages the paths/urls of all images to display.

// Features
#![feature(never_type)]

// Imports
use {
	rand::prelude::SliceRandom,
	std::{collections::HashSet, path::PathBuf, sync::Arc},
	zsw_util::Resources,
};

/// Image playlist
// TODO: Rename to `PlaylistService`
#[derive(Debug)]
pub struct Playlist {
	/// Image sender
	img_tx: async_channel::Sender<Arc<PlaylistImage>>,

	/// Image receiver
	img_rx: async_channel::Receiver<Arc<PlaylistImage>>,
}

impl Playlist {
	/// Creates a new, empty, playlist, alongside all resources
	#[must_use]
	pub fn new() -> (Self, PlaylistResource) {
		// Note: Making the close channel unbounded is what allows us to not block
		//       in `Self::stop`.
		let (img_tx, img_rx) = async_channel::bounded(1);

		// Create the service
		let service = Self { img_tx, img_rx };

		// Create the resource
		let resource = PlaylistResource {
			root_path:  None,
			images:     HashSet::new(),
			cur_images: vec![],
		};

		(service, resource)
	}

	/// Runs the playlist
	///
	/// # Blocking
	/// Locks [`PlaylistLock`] on `self`.
	pub async fn run<R>(&self, resources: &R) -> !
	where
		R: Resources<PlaylistResource>,
	{
		loop {
			// Get the next image to send
			// DEADLOCK: Caller ensures we can lock it
			// Note: It's important to not have this in the match expression, as it would
			//       keep the lock through the whole match.
			let next = resources.resource::<PlaylistResource>().await.cur_images.pop();

			// Then check if we got it
			match next {
				// If we got it, send it
				// DEADLOCK: We don't hold any locks while sending
				// Note: This can't return an `Err` because `self` owns a receiver
				Some(image) => self.img_tx.send(image).await.expect("Image receiver was closed"),

				// Else get the next batch and shuffle them
				// DEADLOCK: Caller ensures we can lock it.
				None => {
					let mut resource = resources.resource::<PlaylistResource>().await;
					let resource = &mut *resource;
					resource.cur_images.extend(resource.images.iter().cloned());
					resource.cur_images.shuffle(&mut rand::thread_rng());
				},
			}
		}
	}

	/// Removes an image
	pub async fn remove_image<'a>(&'a self, resource: &mut PlaylistResource, image: &PlaylistImage) {
		// Note: We don't care if the image actually existed or not
		let _ = resource.images.remove(image);
	}

	/// Sets the root path
	pub async fn set_root_path<'a>(&'a self, resource: &mut PlaylistResource, root_path: PathBuf) {
		// Remove all existing paths and add new ones
		resource.images.clear();
		zsw_util::visit_dir(&root_path, &mut |path| {
			let _ = resource.images.insert(Arc::new(PlaylistImage::File(path)));
		});

		// Remove all current paths too
		resource.cur_images.clear();

		// Save the root path
		resource.root_path = Some(root_path);
	}

	/// Returns the root path
	pub async fn root_path<'a>(&'a self, resource: &'a PlaylistResource) -> Option<PathBuf> {
		resource.root_path.clone()
	}

	/// Retrieves the next image
	///
	/// # Blocking
	/// Locks [`PlaylistLock`] on `??`
	// TODO: Replace `Locks` with a barrier on the channel
	// Note: Doesn't literally lock it, but the other side of the channel
	//       needs to lock it in order to progress, so it's equivalent
	pub async fn next(&self) -> Arc<PlaylistImage> {
		// Note: This can't return an `Err` because `self` owns a sender
		// DEADLOCK: Caller ensures it won't hold an `PlaylistLock`,
		//           and we ensure the other side of the channel
		//           can progress.
		self.img_rx.recv().await.expect("Image sender was closed")
	}

	/// Peeks the next images
	pub async fn peek_next(&self, resource: &PlaylistResource, mut f: impl FnMut(&PlaylistImage) + Send) {
		for image in resource.cur_images.iter().rev() {
			f(image);
		}
	}
}

/// Playlist resource
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct PlaylistResource {
	/// Root path
	// TODO: Use this properly
	root_path: Option<PathBuf>,

	/// All images
	images: HashSet<Arc<PlaylistImage>>,

	/// Current images
	cur_images: Vec<Arc<PlaylistImage>>,
}

/// A playlist image
#[derive(PartialEq, Eq, Clone, Hash, Debug)]
pub enum PlaylistImage {
	/// File path
	File(PathBuf),
	// TODO: URL
}
