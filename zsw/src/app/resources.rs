//! Resources

// Imports
use {
	futures::lock::{Mutex, MutexLockFuture},
	zsw_panels::PanelsResource,
	zsw_playlist::PlaylistResource,
	zsw_util::{ResourcesBundle, ResourcesLock},
};

/// All resources
pub struct Resources {
	/// Panels
	pub panels: Mutex<PanelsResource>,

	/// Playlist
	pub playlist: Mutex<PlaylistResource>,
}

impl ResourcesBundle for Resources {}

#[duplicate::duplicate_item(
	ty                 field;
	[ PanelsResource   ] [ panels ];
	[ PlaylistResource ] [ playlist ];
)]
impl ResourcesLock<ty> for Resources {
	fn lock(&self) -> MutexLockFuture<ty> {
		self.field.lock()
	}
}
