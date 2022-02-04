//! Panels

// Imports
use {crate::Panel, parking_lot::Mutex};

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
		self.panels.lock().push(panel);
	}

	/// Iterates over all panels mutably
	pub fn for_each_mut<T, C: FromIterator<T>>(&self, f: impl FnMut(&mut Panel) -> T) -> C {
		let mut panels = self.panels.lock();
		panels.iter_mut().map(f).collect()
	}
}
