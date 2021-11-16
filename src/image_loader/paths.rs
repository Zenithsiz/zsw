//! Paths channel

// Imports
use parking_lot::{Condvar, Mutex};
use std::{path::PathBuf, sync::Arc};


/// All paths
struct Paths {
	/// All paths
	paths: Vec<PathBuf>,

	/// Current index
	cur_idx: usize,

	/// Paths to remove
	to_remove: Vec<usize>,

	/// Current iteration
	// Note: This is used to synchronize the values `to_remove`, since they're by index.
	cur_it: usize,
}

/// Path receiver
pub struct Receiver {
	/// The paths
	paths: Arc<Mutex<Paths>>,

	/// Modifier cond var
	modifier_cond_var: Arc<Condvar>,

	/// Receiver cond var
	receiver_cond_var: Arc<Condvar>,
}

/// An index into a `Receiver`'s `recv` path
#[derive(Clone, Copy, Debug)]
pub struct RecvIdx {
	/// Iteration of this index
	it: usize,

	/// Index
	idx: usize,
}

impl Receiver {
	/// Retrieves the next path
	pub fn recv(&self) -> (RecvIdx, PathBuf) {
		// Lock the paths
		let mut paths = self.paths.lock();

		loop {
			// Get our index and bump it
			let idx = paths.cur_idx;
			paths.cur_idx += 1;

			// Try to get it
			match paths.paths.get(idx) {
				Some(path) => return (RecvIdx { idx, it: paths.cur_it }, path.clone()),

				// If we didn't get it, let the modifier to go, and wait on the cond var
				None => {
					self.modifier_cond_var.notify_one();
					self.receiver_cond_var.wait(&mut paths);
				},
			}
		}
	}

	/// Removes a path given it's index
	///
	/// If the paths have been changed since, this will be ignored.
	pub fn remove(&self, idx: RecvIdx) {
		// Lock the paths
		let mut paths = self.paths.lock();

		// If it's not from the same iteration, return
		if idx.it != paths.cur_it {
			return;
		}

		// Else add it to the `to_remove`
		paths.to_remove.push(idx.idx);
	}
}

impl Clone for Receiver {
	fn clone(&self) -> Self {
		Self {
			paths:             Arc::clone(&self.paths),
			modifier_cond_var: Arc::clone(&self.modifier_cond_var),
			receiver_cond_var: Arc::clone(&self.receiver_cond_var),
		}
	}
}

/// Path Modifier
pub struct Modifier {
	/// The paths
	paths: Arc<Mutex<Paths>>,

	/// Modifier cond var
	modifier_cond_var: Arc<Condvar>,

	/// Receiver cond var
	receiver_cond_var: Arc<Condvar>,
}

impl Modifier {
	/// Enters a reset-wait loop on this modifier
	pub fn reset_wait(&self, mut f: impl FnMut(&mut Vec<PathBuf>)) -> ! {
		// Lock the paths
		let mut paths_lock = self.paths.lock();

		loop {
			let paths = &mut *paths_lock;

			// For each path we need to remove, remove it
			for idx in paths.to_remove.drain(..) {
				paths.paths.swap_remove(idx);
			}

			// Call `f` and reset the current index
			f(&mut paths.paths);
			paths.cur_idx = 0;
			paths.cur_it += 1;

			// Then wait until all receivers are done
			self.receiver_cond_var.notify_all();
			self.modifier_cond_var.wait(&mut paths_lock);
		}
	}
}

/// Creates a new paths modify-receiver channel with existing paths
pub fn channel(paths: Vec<PathBuf>) -> (Modifier, Receiver) {
	let paths = Arc::new(Mutex::new(Paths {
		paths,
		cur_idx: 0,
		to_remove: vec![],
		cur_it: 0,
	}));

	let modifier_cond_var = Arc::new(Condvar::new());
	let receiver_cond_var = Arc::new(Condvar::new());

	(
		Modifier {
			paths:             Arc::clone(&paths),
			modifier_cond_var: Arc::clone(&modifier_cond_var),
			receiver_cond_var: Arc::clone(&receiver_cond_var),
		},
		Receiver {
			paths,
			modifier_cond_var,
			receiver_cond_var,
		},
	)
}
