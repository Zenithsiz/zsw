//! Locker

// Lints
#![expect(
	clippy::disallowed_methods,
	reason = "DEADLOCK: We ensure thread safety via the locker abstraction"
)]

// Modules
mod meetup;
mod mutex;
mod rwlock;

// Exports
pub use self::{meetup::MeetupSenderResource, mutex::AsyncMutexResource, rwlock::AsyncRwLockResource};

// Imports
use {
	crate::{
		panel::{PanelGroup, PanelsRendererShader, PlaylistPlayer},
		playlist::{Playlist, PlaylistItem, Playlists},
	},
	futures::{stream::FuturesUnordered, Future, Stream, StreamExt},
	std::marker::PhantomData,
};

#[cfg(feature = "locker-validation")]
use {
	dashmap::{mapref::entry::Entry as DashMapEntry, DashMap},
	std::sync::{
		atomic::{self, AtomicU64},
		LazyLock,
	},
};

/// Async locker id
#[cfg(feature = "locker-validation")]
#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
struct AsyncLockerId(u64);

/// Locker associated to a tokio task.
#[cfg(feature = "locker-validation")]
static TASK_LOCKER: LazyLock<DashMap<tokio::task::Id, AsyncLockerId>> = LazyLock::new(DashMap::new);

/// Async locker.
///
/// Per-task entity to enforce lock order and progression
///
/// # Task
/// A task, for this locker, is defined as an entity capable of doing
/// work, separate from all other tasks.
///
/// e.g. A tokio task is a task. Each entry in a `join!()` future is also
///      a task, because each branch can make progress without the other branches
///      affecting it.
///
/// # Order
/// Order is ensured through the `*Resource` traits. These are implemented
/// in a way that once locked, the locker advances it's state. This new state
/// can no longer lock that same resource, and some resources are only lockable
/// at a certain state.
///
/// # Lifetime
/// The `'task` lifetime is associated with the current task. For tokio tasks, this means
/// that it should be the whole task lifetime. While for e.g. `tokio::join()`, it should be
/// each branch future.
#[derive(Debug)]
pub struct AsyncLocker<'task, const STATE: usize> {
	/// Locker id
	#[cfg(feature = "locker-validation")]
	id: AsyncLockerId,

	/// Locker task
	// TODO: Add support for "subtasks" for when splitting the locker.
	#[cfg(feature = "locker-validation")]
	task: tokio::task::Id,

	_prev: PhantomData<&'task Self>,
}

impl AsyncLocker<'_, 0> {
	/// Creates a new locker for this task.
	///
	/// # Panics
	/// Panics if two lockers are created in the same task, or if
	/// created outside of a task.
	#[expect(clippy::new_without_default)] // We want locker creation to be explicit
	pub fn new() -> Self {
		// Create the next id.
		#[cfg(feature = "locker-validation")]
		let id = {
			static ID: AtomicU64 = AtomicU64::new(0);
			let id = ID.fetch_add(1, atomic::Ordering::AcqRel);
			AsyncLockerId(id)
		};

		// Get the current task and check for duplicates
		#[cfg(feature = "locker-validation")]
		let task = match TASK_LOCKER.entry(tokio::task::id()) {
			DashMapEntry::Occupied(entry) => {
				let task = entry.key();
				let other_id = entry.get();
				zsw_util::log_error_panic!(?task, ?id, ?other_id, "Two lockers were created on the same tokio task");
			},
			DashMapEntry::Vacant(entry) => {
				let task = entry.key();
				tracing::trace!(?id, ?task, "Assigned task to locker");
				let entry = entry.insert(id);
				*entry.key()
			},
		};

		Self {
			#[cfg(feature = "locker-validation")]
			id,
			#[cfg(feature = "locker-validation")]
			task,
			_prev: PhantomData,
		}
	}
}

#[cfg_attr(not(feature = "locker-validation"), expect(clippy::unused_self))] // Only required when validating
impl<const STATE: usize> AsyncLocker<'_, STATE> {
	/// Clones this locker for a sub-task
	///
	/// # Deadlock
	/// You must ensure each clone of the locker is able to make
	/// progress on it's own, to avoid deadlocks
	fn clone(&self) -> AsyncLocker<'_, STATE> {
		AsyncLocker {
			#[cfg(feature = "locker-validation")]
			id: self.id,
			#[cfg(feature = "locker-validation")]
			task: self.task,
			_prev: PhantomData,
		}
	}

	/// Creates the next locker
	///
	/// # Deadlock
	/// You must ensure the next state cannot deadlock with the current state.
	fn next<const NEXT_STATE: usize>(&self) -> AsyncLocker<'_, NEXT_STATE> {
		AsyncLocker {
			#[cfg(feature = "locker-validation")]
			id: self.id,
			#[cfg(feature = "locker-validation")]
			task: self.task,
			_prev: PhantomData,
		}
	}

	/// Signals to this locker that we're going to be awaiting.
	///
	/// A few invariants will be checked before returning
	fn start_awaiting(&self) {
		// If we're on a different task, log error
		#[cfg(feature = "locker-validation")]
		{
			let cur_task = tokio::task::id();
			if cur_task != self.task {
				zsw_util::log_error_panic!(?self.id, ?self.task, ?cur_task, "AsyncLocker was used in two different tasks");
			}
		}
	}
}

/// Extension method for streams using lockers
#[extend::ext(name = LockerStreamExt)]
pub impl<S: Stream> S {
	/// Splits a locker across this stream into an unordered stream
	// TODO: Not require `Send` here.
	// TODO: Not require `Fut::Output: 'static` and instead make `F` generic over `'cur`.
	fn split_locker_async_unordered<'cur, F, Fut, const STATE: usize>(
		self,
		locker: &'cur mut AsyncLocker<'_, STATE>,
		mut f: F,
	) -> impl Stream<Item = Fut::Output> + 'cur
	where
		S: 'cur,
		F: FnMut(S::Item, AsyncLocker<'cur, STATE>) -> Fut + 'cur,
		Fut: Future<Output: 'static> + 'cur,
	{
		// DEADLOCK: Each future in `buffer_unordered` can make progress
		//           on it's own.
		let locker = &*locker;
		self.map(move |item| f(item, locker.clone()))
			.buffer_unordered(usize::MAX)
	}
}

/// Extension method for iterators using lockers
#[extend::ext(name = LockerIteratorExt)]
pub impl<I: Iterator> I {
	/// Splits a locker across this iterator into an unordered stream
	// TODO: Not require `Send` here.
	// TODO: Not require `Fut::Output: 'static` and instead make `F` generic over `'cur`.
	fn split_locker_async_unordered<'cur, F, Fut, const STATE: usize>(
		self,
		locker: &'cur mut AsyncLocker<'_, STATE>,
		mut f: F,
	) -> impl Stream<Item = Fut::Output> + 'cur
	where
		F: FnMut(I::Item, AsyncLocker<'cur, STATE>) -> Fut + 'cur,
		Fut: Future<Output: 'static> + 'cur,
	{
		// DEADLOCK: Each future in `FuturesUnordered` can make progress
		//           on it's own.
		let locker = &*locker;
		self.map(move |item| f(item, locker.clone()))
			.collect::<FuturesUnordered<_>>()
	}
}

locker_impls! {
	inner;
	fn new(...) -> ...;

	async_mutex {
		CurPanelGroupMutex(Option<PanelGroup>) {
			0 => 1,
		},
	}

	async_rwlock {
		PlaylistsRwLock(Playlists) {
			0 => 1,
		},
		PlaylistRwLock(Playlist) {
			0 => 1,
		},
		PlaylistItemRwLock(PlaylistItem) {
			0, 1 => 2,
		},
		PanelsRendererShaderRwLock(PanelsRendererShader) {
			0, 1 => 2,
		},
		PlaylistPlayerRwLock(PlaylistPlayer) {
			0, 1 => 2,
		},
	}

	meetup_sender {
		PanelsUpdaterMeetupSender(()) {
			0,
		},
		EguiPainterRendererMeetupSender((Vec<egui::ClippedPrimitive>, egui::TexturesDelta)) {
			0,
		},
	}
}

macro locker_impls(
	$inner:ident;
	fn $new:ident(...) -> ...;

	async_mutex {
		$(
			$AsyncMutexName:ident($AsyncMutexInner:ty) {
				$( $( $ASYNC_MUTEX_CUR_STATE:literal ),* $(,)? => $ASYNC_MUTEX_NEXT_STATE:literal ),*
				$(,)?
			}
		),*
		$(,)?
	}

	async_rwlock {
		$(
			$AsyncRwLockName:ident($AsyncRwLockInner:ty) {
				$( $( $ASYNC_RWLOCK_CUR_STATE:literal ),* $(,)? => $ASYNC_RWLOCK_NEXT_STATE:literal ),*
				$(,)?
			}
		),*
		$(,)?
	}

	meetup_sender {
		$(
			$MeetupName:ident($MeetupInner:ty) {
				$( $MEETUP_CUR_STATE:literal ),*
				$(,)?
			}
		),*
		$(,)?
	}
) {
	$(
		mutex::resource_impl! {
			$AsyncMutexName { $inner: $AsyncMutexInner };
			fn $new(...) -> ...;

			states {
				$(
					$(
						$ASYNC_MUTEX_CUR_STATE => $ASYNC_MUTEX_NEXT_STATE,
					)*
				)*
			}
		}
	)*

	$(
		rwlock::resource_impl! {
			$AsyncRwLockName { $inner: $AsyncRwLockInner };
			fn $new(...) -> ...;

			states {
				$(
					$(
						$ASYNC_RWLOCK_CUR_STATE => $ASYNC_RWLOCK_NEXT_STATE,
					)*
				)*
			}
		}
	)*

	$(
		meetup::resource_impl! {
			$MeetupName { $inner: $MeetupInner };
			fn $new(...) -> ...;

			states {
				$( $MEETUP_CUR_STATE, )*
			}
		}
	)*
}
