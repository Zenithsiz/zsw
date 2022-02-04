//! Distributer

// Imports
use {
	super::Inner,
	crate::util::{
		extse::{CrossBeamChannelSenderSE, ParkingLotMutexSe},
		MightBlock,
	},
	parking_lot::Mutex,
	rand::prelude::SliceRandom,
	std::{
		collections::HashSet,
		mem,
		path::{Path, PathBuf},
		sync::Arc,
	},
	zsw_side_effect_macros::side_effect,
};

/// The distributer
///
/// This type may not be cloned, only a single instance
/// exists per channel.
#[derive(Debug)]
pub struct Distributer {
	/// Inner
	// DEADLOCK: We ensure this lock can't deadlock by not blocking
	//           while locked.
	pub(super) inner: Arc<Mutex<Inner>>,

	/// Path sender
	pub(super) tx: crossbeam::channel::Sender<Arc<PathBuf>>,
}

impl Distributer {
	/// Runs the distributer until all receivers have quit
	///
	/// # Blocking
	/// Blocks until a receiver receives via [`Receiver::recv`](`super::Receiver::recv`).
	#[side_effect(MightBlock)]
	pub fn run(&self) -> Result<(), anyhow::Error> {
		let mut cur_paths = vec![];
		'run: loop {
			// DEADLOCK: We ensure this lock can't deadlock by not blocking
			//           while locked.
			let mut inner = self.inner.lock_se().allow::<MightBlock>();

			// If we need to reload, clear and load the paths
			let cur_root_path = Arc::clone(&inner.root_path);
			if inner.reload_cached {
				inner.cached_paths.clear();
				Self::load_paths_into(&cur_root_path, &mut inner.cached_paths);
				inner.reload_cached = false;
			}

			// Copy all paths and shuffle
			// Note: We also drop the lock after copying, since we don't need
			//       inner anymore
			// DEADLOCK: We drop the lock here, so calls to `self.tx.send` aren't under the lock
			cur_paths.extend(inner.cached_paths.iter().cloned());
			mem::drop(inner);
			cur_paths.shuffle(&mut rand::thread_rng());

			// Then send them all
			for path in cur_paths.drain(..) {
				// If the root path changed in the meantime, reload
				// DEADLOCK: We ensure this lock can't deadlock by not blocking
				//           while locked.
				// TODO: `ptr_eq` here? Not sure if it's worth it
				if self.inner.lock_se().allow::<MightBlock>().root_path != cur_root_path {
					log::debug!("Root path changed, resetting");
					continue 'run;
				}

				// Else send the path
				// DEADLOCK: Caller is responsible for avoiding deadlocks
				if self.tx.send_se(path).allow::<MightBlock>().is_err() {
					log::debug!("All receivers quit, quitting");
					break 'run;
				}
			}
		}

		Ok(())
	}

	/// Loads all paths
	fn load_paths_into(root_path: &Path, paths: &mut HashSet<Arc<PathBuf>>) {
		log::info!("Loading all paths from {root_path:?}");

		let ((), duration) = crate::util::measure(|| {
			crate::util::visit_files_dir(root_path, &mut |path| {
				let _ = paths.insert(Arc::new(path));
				Ok::<(), !>(())
			})
			.into_ok();
		});
		log::trace!(target: "zsw::perf", "Took {duration:?} to load all paths from {root_path:?}");
	}

	/// Returns the current root path
	pub fn root_path(&self) -> Arc<PathBuf> {
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked.
		Arc::clone(&self.inner.lock_se().allow::<MightBlock>().root_path)
	}

	/// Sets the root path
	pub fn set_root_path(&self, path: PathBuf) {
		log::info!("Setting root path to {path:?}");

		// Set the root path and clear all paths
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked.
		let mut inner = self.inner.lock_se().allow::<MightBlock>();
		inner.root_path = Arc::new(path);
		inner.reload_cached = true;
	}
}
