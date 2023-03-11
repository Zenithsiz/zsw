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
	futures::{stream::FuturesUnordered, Future, Stream, StreamExt},
	std::sync::{
		atomic::{self, AtomicU64},
		OnceLock,
	},
	zsw_util::Oob,
};

/// Async locker inner
#[derive(Debug)]
struct LockerInner {
	/// Locker id
	id: u64,

	/// Current cell
	cur_task: OnceLock<tokio::task::Id>,
}

/// Async locker.
///
/// Ensures two tasks don't deadlock when locking resources.
// TODO: Simplify to `AsyncLocker(())` on release builds?
#[derive(Debug)]
pub struct AsyncLocker<'prev, const STATE: usize> {
	inner: Oob<'prev, LockerInner>,
}

impl<'prev> AsyncLocker<'prev, 0> {
	/// Creates a new locker
	///
	/// # Deadlock
	/// You must ensure only a single locker exists per-task, under stacked borrows
	pub fn new() -> Self {
		// Create the next id.
		static ID: AtomicU64 = AtomicU64::new(0);
		let id = ID.fetch_add(1, atomic::Ordering::AcqRel);

		Self {
			inner: Oob::Owned(LockerInner {
				id,
				cur_task: OnceLock::new(),
			}),
		}
	}
}

impl<'prev, const STATE: usize> AsyncLocker<'prev, STATE> {
	/// Clones this locker
	///
	/// # Deadlock
	/// You must ensure only a single locker exists per-task, under stacked borrows
	fn clone(&self) -> AsyncLocker<'_, STATE> {
		AsyncLocker {
			inner: self.inner.to_borrowed(),
		}
	}

	/// Creates the next locker
	///
	/// # Deadlock
	/// You must ensure only a single locker exists per-task, under stacked borrows
	fn next<const NEXT_STATE: usize>(&self) -> AsyncLocker<'_, NEXT_STATE> {
		AsyncLocker {
			inner: self.inner.to_borrowed(),
		}
	}

	/// Ensures the locker hasn't escaped it's initial task
	fn ensure_same_task(&self) {
		// Get the task, or initialize it
		let task = self.inner.cur_task.get_or_init(|| {
			let cur_task = tokio::task::id();
			tracing::trace!(locker_id = ?self.inner.id, ?cur_task,"Assigned task to locker");
			cur_task
		});

		// Then check if we're on a different task
		let cur_task = tokio::task::id();
		if cur_task != *task {
			tracing::error!(locker_id = ?self.inner.id, ?task, ?cur_task, "AsyncLocker was used in two different tasks");
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
