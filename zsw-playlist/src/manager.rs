//! Playlist Manager

// Imports
use {
	crate::{Event, PlaylistImage, PlaylistInner},
	crossbeam::channel,
	parking_lot::RwLock,
	std::{path::PathBuf, sync::Arc},
};

/// Playlist manager
#[derive(Clone, Debug)]
pub struct PlaylistManager {
	/// Inner
	pub(crate) inner: Arc<RwLock<PlaylistInner>>,

	/// Event sender
	pub(crate) event_tx: channel::Sender<Event>,
}

impl PlaylistManager {
	/// Removes an image
	pub fn remove_image(&mut self, image: Arc<PlaylistImage>) {
		// TODO: Care about this?
		let _res = self.event_tx.send(Event::RemoveImg(image));
	}

	/// Sets the root path
	pub fn set_root_path(&mut self, root_path: impl Into<PathBuf>) {
		// TODO: Care about this?
		let _res = self.event_tx.send(Event::ChangeRoot(root_path.into()));
	}

	/// Returns the root path
	#[must_use]
	pub fn root_path(&mut self) -> Option<PathBuf> {
		self.inner.read().root_path.clone()
	}

	/// Returns the remaining images in the current shuffle
	#[must_use]
	pub fn peek_next(&mut self) -> Vec<Arc<PlaylistImage>> {
		self.inner.read().cur_images.clone()
	}
}
