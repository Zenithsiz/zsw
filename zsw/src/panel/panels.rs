//! Panels

// Imports
use {
	crate::{
		util::{extse::ParkingLotMutexSe, MightBlock},
		Panel,
	},
	parking_lot::Mutex,
	zsw_side_effect_macros::side_effect,
};

/// All panels
#[derive(Debug)]
pub struct Panels {
	/// All of the panels
	panels: Mutex<Vec<Panel>>,
}

impl Panels {
	/// Creates the panel
	pub fn new(panels: impl IntoIterator<Item = Panel>) -> Self {
		Self {
			panels: Mutex::new(panels.into_iter().collect()),
		}
	}

	/// Adds a new panel
	pub fn add_panel(&self, panel: Panel) {
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked.
		self.panels.lock_se().allow::<MightBlock>().push(panel);
	}

	/// Iterates over all panels mutably.
	///
	/// # Blocking
	/// Will deadlock if `f` blocks.
	#[side_effect(MightBlock)]
	pub fn for_each_mut<T, C: FromIterator<T>>(&self, f: impl FnMut(&mut Panel) -> T) -> C {
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked.
		//           Caller ensures `f` won't block
		let mut panels = self.panels.lock_se().allow::<MightBlock>();
		panels.iter_mut().map(f).collect()
	}
}
