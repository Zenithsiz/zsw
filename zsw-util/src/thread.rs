//! Threading utilities

// Imports
use {
	anyhow::Context,
	std::thread::{self, Scope, ScopedJoinHandle},
};

/// Thread spawner.
///
/// Thin wrapper around `std::thread::Scope` to spawn threads
/// with a given name, as well as save all join handles, to
/// be able to join them all later.
#[derive(Debug)]
pub struct ThreadSpawner<'scope, 'env, T> {
	/// Scope
	scope: &'scope Scope<'scope, 'env>,

	/// All join handles along with the thread names
	join_handles: Vec<ScopedJoinHandle<'scope, T>>,
}

impl<'scope, 'env, T> ThreadSpawner<'scope, 'env, T> {
	/// Creates a new thread spawner
	pub fn new(scope: &'scope Scope<'scope, 'env>) -> Self {
		Self {
			scope,
			join_handles: vec![],
		}
	}

	/// Spawns a new thread
	pub fn spawn<F>(&mut self, name: impl Into<String>, f: F) -> Result<(), anyhow::Error>
	where
		F: Send + FnOnce() -> T + 'env,
		T: Send + 'env,
	{
		let name = name.into();
		let handle = thread::Builder::new()
			.name(name.clone())
			.spawn_scoped(self.scope, f)
			.with_context(|| format!("Unable to spawn thread {name:?}"))?;
		self.join_handles.push(handle);

		Ok(())
	}

	/// Joins all threads
	pub fn join_all<C: FromIterator<T>>(self) -> Result<C, anyhow::Error> {
		// Note: We only join in reverse order because that's usually the order
		//       the threads will stop, nothing else. No sequencing exists.
		self.join_handles
			.into_iter()
			.rev()
			.map(|handle| {
				let name = handle.thread().name().unwrap_or("<unnamed>").to_owned();
				tracing::debug!(?name, "Joining thread");
				handle
					.join()
					.map_err(|err| anyhow::anyhow!("Thread '{name}' panicked at {err:?}"))
			})
			.collect()
	}
}
