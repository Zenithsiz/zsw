//! Panel state

// Imports
use {super::PanelImageId, crate::Panel};

/// Panel state
#[derive(Debug)]
pub struct PanelState {
	/// Panel
	pub panel: Panel,

	/// Images
	pub images: PanelStateImages,

	/// Current progress (in frames)
	pub cur_progress: u64,
}

impl PanelState {
	/// Creates a new panel
	#[must_use]
	pub const fn new(panel: Panel) -> Self {
		Self {
			panel,
			images: PanelStateImages::Empty,
			cur_progress: 0,
		}
	}
}

/// Images for a panel state
#[derive(Clone, Default, Debug)]
pub enum PanelStateImages {
	/// Empty
	///
	/// This means no images have been loaded yet
	#[default]
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
#[derive(Clone, Debug)]
#[allow(missing_copy_implementations)] // We don't want it to be trivially copyable yet because it manages a resource
pub struct PanelImageStateImage {
	/// Image id
	pub id: PanelImageId,

	/// If swapping directions
	pub swap_dir: bool,
}
