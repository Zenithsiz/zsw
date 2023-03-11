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
		panel::{PanelGroup, PanelsRendererShader},
		playlist::{Playlist, PlaylistItem, Playlists},
	},
	dashmap::{mapref::entry::Entry as DashMapEntry, DashMap},
	futures::{stream::FuturesUnordered, Future, Stream, StreamExt},
	std::{
		marker::PhantomData,
		sync::{
			atomic::{self, AtomicU64},
			LazyLock,
		},
	},
};

/// Async locker id
#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
struct AsyncLockerId(u64);

/// Locker associated to a tokio task.
static TASK_LOCKER: LazyLock<DashMap<tokio::task::Id, AsyncLockerId>> = LazyLock::new(DashMap::new);

/// Async locker.
///
/// Ensures two tasks don't deadlock when locking resources.
// TODO: Simplify to `AsyncLocker(())` on release builds?
#[derive(Debug)]
pub struct AsyncLocker<'prev, const STATE: usize> {
	/// Locker id
	id: AsyncLockerId,

	/// Locker task
	// TODO: Add support for "subtasks" for when splitting the locker.
	task: tokio::task::Id,

	_prev: PhantomData<&'prev Self>,
}

impl<'prev> AsyncLocker<'prev, 0> {
	/// Creates a new locker for this task.
	///
	/// # Panics
	/// Panics if two lockers are created in the same task, or if
	/// created outside of a task.
	pub fn new() -> Self {
		// Create the next id.
		static ID: AtomicU64 = AtomicU64::new(0);
		let id = ID.fetch_add(1, atomic::Ordering::AcqRel);
		let id = AsyncLockerId(id);

		// Get the current task
		let task = tokio::task::id();
		match TASK_LOCKER.entry(task) {
			DashMapEntry::Occupied(entry) => {
				let other_id = entry.get();
				zsw_util::log_error_panic!(?task, ?id, ?other_id, "Two lockers were created on the same tokio task");
			},
			DashMapEntry::Vacant(entry) => {
				tracing::trace!(?id, ?task, "Assigned task to locker");
				let _ = entry.insert(id);
			},
		};

		Self {
			id,
			task,
			_prev: PhantomData,
		}
	}
}

impl<'prev, const STATE: usize> AsyncLocker<'prev, STATE> {
	/// Clones this locker for a sub-task
	///
	/// # Deadlock
	/// You must ensure each clone of the locker is able to make
	/// progress on it's own, to avoid deadlocks
	fn clone(&self) -> AsyncLocker<'_, STATE> {
		AsyncLocker {
			id:    self.id,
			task:  self.task,
			_prev: PhantomData,
		}
	}

	/// Creates the next locker
	///
	/// # Deadlock
	/// You must ensure the next state cannot deadlock with the current state.
	fn next<const NEXT_STATE: usize>(&self) -> AsyncLocker<'_, NEXT_STATE> {
		AsyncLocker {
			id:    self.id,
			task:  self.task,
			_prev: PhantomData,
		}
	}

	/// Signals to this locker that we're going to be awaiting.
	///
	/// A few invariants will be checked before returning
	fn start_awaiting(&self) {
		// If we're on a different task, log error
		let cur_task = tokio::task::id();
		if cur_task != self.task {
			zsw_util::log_error_panic!(?self.id, ?self.task, ?cur_task, "AsyncLocker was used in two different tasks");
		}
	}
}

/// Extension method for streams using lockers
#[extend::ext(name = LockerStreamExt)]
pub impl<S: Stream> S {
	/// Splits a locker across this stream into an unordered stream
	// TODO: Not require `Send` here.
	// TODO: Not require `Fut::Output: 'static` and instead make `F` generic over `'cur`.
	fn split_locker_async_unordered<'prev, 'cur, F, Fut, const STATE: usize>(
		self,
		locker: &'cur mut AsyncLocker<'prev, STATE>,
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
	fn split_locker_async_unordered<'prev, 'cur, F, Fut, const STATE: usize>(
		self,
		locker: &'cur mut AsyncLocker<'prev, STATE>,
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
			0 => 1,
		},
		PanelsRendererShaderRwLock(PanelsRendererShader) {
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
