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
	futures::{stream::FuturesUnordered, Future, Stream},
	std::{
		ops::Deref,
		sync::{
			atomic::{self, AtomicU64},
			OnceLock,
		},
	},
};

/// Current task
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum CurTask {
	/// Thread
	Thread(std::thread::ThreadId),

	/// Tokio Task
	TokioTask(tokio::task::Id),
}

impl CurTask {
	/// Returns the current task id.
	///
	/// If within a tokio task, returns `TokioTask`, else returns the thread id
	pub fn get() -> Self {
		if let Some(id) = tokio::task::try_id() {
			return Self::TokioTask(id);
		}

		Self::Thread(std::thread::current().id())
	}
}

/// Locker inner
#[derive(Debug)]
struct LockerInner {
	/// Locker id
	id: u64,

	/// Current task
	///
	/// Will be `None` if the locker hasn't been used yet.
	cur_task: OnceLock<CurTask>,
}

/// Locker inner kind
#[derive(Debug)]
enum LockerInnerKind<'prev> {
	Owned(LockerInner),
	Borrowed(&'prev LockerInner),
}

impl<'prev> Deref for LockerInnerKind<'prev> {
	type Target = LockerInner;

	fn deref(&self) -> &Self::Target {
		match self {
			LockerInnerKind::Owned(inner) => inner,
			LockerInnerKind::Borrowed(inner) => inner,
		}
	}
}

/// Locker
#[derive(Debug)]
// TODO: Simplify to `Locker(())` on release builds?
pub struct Locker<'prev, const STATE: usize> {
	inner: LockerInnerKind<'prev>,
}

impl<'prev> Locker<'prev, 0> {
	/// Creates a new locker
	///
	/// # Deadlock
	/// You must ensure only a single locker exists per-task, under stacked borrows
	pub fn new() -> Self {
		static ID: AtomicU64 = AtomicU64::new(0);

		let id = ID.fetch_add(1, atomic::Ordering::AcqRel);
		let inner = LockerInner {
			id,
			cur_task: OnceLock::new(),
		};
		Self {
			inner: LockerInnerKind::Owned(inner),
		}
	}
}

impl<'prev, const STATE: usize> Locker<'prev, STATE> {
	/// Clones this locker
	///
	/// # Deadlock
	/// You must ensure only a single locker exists per-task, under stacked borrows
	fn clone(&self) -> Locker<'_, STATE> {
		Locker {
			inner: LockerInnerKind::Borrowed(&self.inner),
		}
	}

	/// Creates the next locker
	///
	/// # Deadlock
	/// You must ensure only a single locker exists per-task, under stacked borrows
	fn next<const NEXT_STATE: usize>(&self) -> Locker<'_, NEXT_STATE> {
		Locker {
			inner: LockerInnerKind::Borrowed(&self.inner),
		}
	}

	/// Ensures the locker hasn't escaped it's initial task
	fn ensure_same_task(&self) {
		// Get the task, or initialize it
		let task = self.inner.cur_task.get_or_init(|| {
			let cur_task = CurTask::get();
			tracing::trace!(locker_id = ?self.inner.id, ?cur_task,"Assigned task to locker");
			cur_task
		});

		// Then check if we're on a different task
		let cur_task = CurTask::get();
		if cur_task != *task {
			tracing::error!(?task, ?cur_task, "Locker was used in two different tasks");
		}
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
		locker: &'cur mut Locker<'prev, STATE>,
		mut f: F,
	) -> impl Stream<Item = Fut::Output> + 'cur
	where
		F: FnMut(I::Item, Locker<'cur, STATE>) -> Fut + 'cur,
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
