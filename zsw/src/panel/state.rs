//! Panel state

// Imports
use super::PanelImageId;

/// Panel state
#[derive(Debug)]
#[allow(missing_copy_implementations)] // We don't want it to be trivially copyable yet because it manages a resource
pub struct PanelState {
	/// Images
	pub images: PanelStateImages,

	/// Current progress (in frames)
	pub cur_progress: u64,
}

impl PanelState {
	/// Creates a new panel
	#[must_use]
	pub const fn new() -> Self {
		Self {
			images:       PanelStateImages::Empty,
			cur_progress: 0,
		}
	}
}

impl Default for PanelState {
	fn default() -> Self {
		Self::new()
	}
}

/// Images for a panel state
#[derive(Clone, Copy, Debug)]
pub enum PanelStateImages {
	/// Empty
	///
	/// This means no images have been loaded yet
	Empty,

	/// Primary only
	///
	/// The primary image is loaded. The back image is still not available
	PrimaryOnly {
		/// Image
		front: PanelImageStateImage,
	},

	/// Both
	///
	/// Both images are loaded to be faded in between
	Both {
		/// Front image
		front: PanelImageStateImage,

		/// Back image
		back: PanelImageStateImage,
	},
}

/// Panel image state image
#[derive(Clone, Copy, Debug)]
pub struct PanelImageStateImage {
	/// Image id
	pub id: PanelImageId,

	/// If swapping directions
	pub swap_dir: bool,
}
