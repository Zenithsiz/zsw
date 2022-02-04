//! Threading utilities

// Imports
use {
	anyhow::Context,
	crossbeam::thread::{Scope, ScopedJoinHandle},
};

/// Thread spawned
// TODO: Allow spawning and retrieving a value
pub struct ThreadSpawner<'scope, 'env> {
	/// Scope
	scope: &'scope Scope<'env>,

	/// All join handles along with the thread names
	join_handles: Vec<ScopedJoinHandle<'scope, Result<(), anyhow::Error>>>,
}

impl<'scope, 'env> ThreadSpawner<'scope, 'env> {
	/// Creates a new thread spawner
	pub fn new(scope: &'scope Scope<'env>) -> Self {
		Self {
			scope,
			join_handles: vec![],
		}
	}

	/// Spawns a new thread using `crossbeam::thread::Scope` with name
	pub fn spawn_scoped<F>(&mut self, name: impl Into<String>, f: F) -> Result<(), anyhow::Error>
	where
		F: Send + FnOnce() -> Result<(), anyhow::Error> + 'env,
	{
		let name = name.into();
		let handle = self
			.scope
			.builder()
			.name(name.clone())
			.spawn(|_| f())
			.with_context(|| format!("Unable to start thread {name:?}"))?;
		self.join_handles.push(handle);

		Ok(())
	}

	/// Spawns multiple scoped threads
	pub fn spawn_scoped_multiple<F>(
		&mut self,
		name: impl Into<String>,
		threads: impl Iterator<Item = F>,
	) -> Result<(), anyhow::Error>
	where
		F: Send + FnOnce() -> Result<(), anyhow::Error> + 'env,
	{
		let name = name.into();
		threads
			.enumerate()
			.try_for_each(move |(idx, f)| self.spawn_scoped(format!("{name}${idx}"), f))
	}

	/// Joins all threads
	pub fn join_all(self) -> Result<(), anyhow::Error> {
		// Note: We only join in reverse order because that's usually the order
		//       the threads will stop, nothing else. No sequencing exists.
		self.join_handles.into_iter().rev().try_for_each(|handle| {
			let name = handle.thread().name().unwrap_or("<unnamed>").to_owned();
			log::debug!("Joining thread '{name:?}'");
			handle
				.join()
				.map_err(|err| anyhow::anyhow!("Thread '{name}' panicked at {err:?}"))?
				.map_err(|err| anyhow::anyhow!("Thread '{name}' returned `Err`: {err:?}"))
		})
	}
}
