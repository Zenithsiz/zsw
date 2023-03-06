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
pub use self::{meetup::MeetupSenderResourceExt, mutex::AsyncMutexResourceExt, rwlock::AsyncRwLockResourceExt};

// Imports
use {
	crate::{
		panel::{PanelGroup, PanelsRendererShader},
		playlist::Playlists,
	},
	std::{
		ops::{Deref, DerefMut},
		sync::atomic::{self, AtomicU64},
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
	cur_task: Option<CurTask>,
}

/// Locker inner kind
#[derive(Debug)]
enum LockerInnerKind<'locker> {
	Owned(LockerInner),
	Borrowed(&'locker mut LockerInner),
}

impl<'locker> Deref for LockerInnerKind<'locker> {
	type Target = LockerInner;

	fn deref(&self) -> &Self::Target {
		match self {
			LockerInnerKind::Owned(inner) => inner,
			LockerInnerKind::Borrowed(inner) => inner,
		}
	}
}

impl<'locker> DerefMut for LockerInnerKind<'locker> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		match self {
			LockerInnerKind::Owned(inner) => inner,
			LockerInnerKind::Borrowed(inner) => inner,
		}
	}
}

/// Locker
#[derive(Debug)]
// TODO: Simplify to `Locker(())` on release builds?
pub struct Locker<'locker, const STATE: usize> {
	inner: LockerInnerKind<'locker>,
}

impl<'locker> Locker<'locker, 0> {
	/// Creates a new locker
	///
	/// # Deadlock
	/// You must ensure only a single locker exists per-task, under stacked borrows
	pub fn new() -> Self {
		static ID: AtomicU64 = AtomicU64::new(0);

		let id = ID.fetch_add(1, atomic::Ordering::AcqRel);
		let inner = LockerInner { id, cur_task: None };
		Self {
			inner: LockerInnerKind::Owned(inner),
		}
	}
}

impl<'locker, const STATE: usize> Locker<'locker, STATE> {
	/// Creates the next locker
	///
	/// # Deadlock
	/// You must ensure only a single locker exists per-task, under stacked borrows
	fn next<const NEXT_STATE: usize>(&mut self) -> Locker<'_, NEXT_STATE> {
		Locker {
			inner: LockerInnerKind::Borrowed(&mut self.inner),
		}
	}

	/// Ensures the locker hasn't escaped it's initial task
	fn ensure_same_task(&mut self) {
		match self.inner.cur_task {
			Some(task) => {
				let cur_task = CurTask::get();
				if cur_task != task {
					tracing::error!(?task, ?cur_task, "Locker was used in two different tasks");
				}
			},

			// If we don't have a current task, set it to the current one
			None => {
				let cur_task = CurTask::get();
				tracing::trace!(locker_id = ?self.inner.id, ?cur_task,"Assigned task to locker");
				self.inner.cur_task = Some(cur_task);
			},
		}
	}
}


locker_impls! {
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
			$AsyncMutexName { inner: $AsyncMutexInner };
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
			$AsyncRwLockName { inner: $AsyncRwLockInner };
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
			$MeetupName { inner: $MeetupInner };
			fn $new(...) -> ...;

			states {
				$( $MEETUP_CUR_STATE, )*
			}
		}
	)*
}
