//! Resources

// Imports
use {
	futures::lock::{Mutex, MutexLockFuture},
	zsw_panels::PanelsResource,
	zsw_util::{ResourcesBundle, ResourcesLock},
};

/// All resources
pub struct Resources {
	/// Panels
	pub panels: Mutex<PanelsResource>,
}

impl ResourcesBundle for Resources {}

impl ResourcesLock<PanelsResource> for Resources {
	fn lock(&self) -> MutexLockFuture<PanelsResource> {
		self.panels.lock()
	}
}
