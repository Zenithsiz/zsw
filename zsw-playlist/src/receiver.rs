//! Playlist image receiver

// Imports
use {
	crate::{Event, PlaylistImage},
	crossbeam::channel,
	std::sync::Arc,
};

/// Playlist receiver
///
/// Unicast-channel to receive images from the playlist service
#[derive(Clone, Debug)]
pub struct PlaylistReceiver {
	/// Image receiver
	pub(crate) img_rx: channel::Receiver<Arc<PlaylistImage>>,

	/// Event sender
	pub(crate) event_tx: channel::Sender<Event>,
}

impl PlaylistReceiver {
	/// Retrieves the next image.
	///
	/// Returns `None` if the playlist manager was closed
	#[must_use]
	pub fn next(&self) -> Option<Arc<PlaylistImage>> {
		self.img_rx.recv().ok()
	}

	/// Removes an image
	pub fn remove_image(&self, image: Arc<PlaylistImage>) {
		// TODO: Care about this?
		let _res = self.event_tx.send(Event::RemoveImg(image));
	}
}
