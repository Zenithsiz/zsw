//! Playlist service runner

// Imports
use {
	crate::{Event, PlaylistImage, PlaylistInner},
	crossbeam::channel,
	parking_lot::RwLock,
	rand::prelude::SliceRandom,
	std::{mem, sync::Arc},
};

/// Playlist runner
///
/// Responsible for driving the
/// playlist receiver
#[derive(Debug)]
pub struct PlaylistRunner {
	/// Inner
	pub(crate) inner: Arc<RwLock<PlaylistInner>>,

	/// Image receiver
	pub(crate) img_rx: channel::Receiver<Arc<PlaylistImage>>,

	/// Image sender
	pub(crate) img_tx: channel::Sender<Arc<PlaylistImage>>,

	/// Event receiver
	pub(crate) event_rx: channel::Receiver<Event>,
}

impl PlaylistRunner {
	/// Runs the playlist runner.
	///
	/// Returns once the playlist receiver is dropped
	pub fn run(self) {
		'run: loop {
			// Lock the inner data for writing
			let mut inner = self.inner.write();

			match inner.all_images.is_empty() {
				// If we have no images, block on the next event
				true => {
					// Note: Important we drop this before blocking
					mem::drop(inner);

					let event = match self.event_rx.recv() {
						Ok(event) => event,
						Err(channel::RecvError) => break 'run,
					};
					let mut inner = self.inner.write();
					self.handle_event(&mut inner, event);
					continue;
				},

				// Else just consume all events we have
				false => loop {
					let event = match self.event_rx.try_recv() {
						Ok(event) => event,
						Err(channel::TryRecvError::Empty) => break,
						Err(channel::TryRecvError::Disconnected) => break 'run,
					};
					self.handle_event(&mut inner, event);
				},
			}

			// Check if we have a next image to send
			match inner.cur_images.pop() {
				// If we got it, send it.
				// Note: It's important to drop the lock while awaiting
				// Note: If the sender got closed in the meantime, quit
				Some(image) => {
					mem::drop(inner);
					if self.img_tx.send(image).is_err() {
						break;
					}
				},

				// Else get the next batch and shuffle them
				// Note: If we got here, `all_images` has at least 1 image,
				//       so we won't be "spin locking" here.
				None => {
					let inner = &mut *inner;

					tracing::debug!("Reshuffling playlist");
					inner.cur_images.extend(inner.all_images.iter().cloned());
					inner.cur_images.shuffle(&mut rand::thread_rng());
				},
			}
		}
	}

	/// Handles event `event`
	fn handle_event(&self, inner: &mut PlaylistInner, event: Event) {
		match event {
			// Note: We don't care if the image actually existed or not
			Event::RemoveImg(image) => {
				let _ = inner.all_images.remove(&image);
			},

			Event::ChangeRoot(root_path) => {
				// Remove all existing paths and add new ones
				inner.all_images.clear();
				zsw_util::visit_dir(&root_path, &mut |path| {
					let _ = inner.all_images.insert(Arc::new(PlaylistImage::File(path)));
				});

				// Remove all current paths too
				inner.cur_images.clear();

				// Flush all images in the channel buffer
				while self.img_rx.try_recv().is_ok() {}

				// Save the root path
				inner.root_path = Some(root_path);
			},
		};
	}
}
