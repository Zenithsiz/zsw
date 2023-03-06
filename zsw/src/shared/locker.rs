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
use crate::{
	panel::{PanelGroup, PanelsRendererShader},
	playlist::Playlists,
};

/// Locker
#[derive(Debug)]
pub struct Locker<const STATE: usize = 0>(());

impl Locker<0> {
	/// Creates a new locker
	///
	/// # Deadlock
	/// You should not create two lockers per-task
	// TODO: Make sure two aren't created in the same task?
	pub fn new() -> Self {
		Self(())
	}
}


locker_impls! {
	fn new(...) -> ...;

	async_mutex {
		CurPanelGroupMutex(Option<PanelGroup>) = [ 0 ] => 1,
	}

	async_rwlock {
		PlaylistsRwLock(Playlists) = [ 0 ] => 1,
		PanelsRendererShaderRwLock(PanelsRendererShader) = [ 0 1 ] => 2,
	}

	meetup_sender {
		PanelsUpdaterMeetupSender(()) = [ 0 ],
		EguiPainterRendererMeetupSender((Vec<egui::ClippedPrimitive>, egui::TexturesDelta)) = [ 0 ],
	}
}

macro locker_impls(
	fn $new:ident(...) -> ...;

	async_mutex {
		$( $async_mutex_name:ident($async_mutex_ty:ty) = [ $( $async_mutex_cur:literal )* ] => $async_mutex_next:literal ),* $(,)?
	}

	async_rwlock {
		$( $async_rwlock_name:ident($async_rwlock_ty:ty) = [ $( $async_rwlock_cur:literal )* ] => $async_rwlock_next:literal ),* $(,)?
	}

	meetup_sender {
		$( $meetup_sender_name:ident($meetup_sender_ty:ty) = [ $( $meetup_sender_cur:literal )* ] ),* $(,)?
	}
) {
	$(
		mutex::resource_impl! {
			$async_mutex_name($async_mutex_ty);
			fn $new(...) -> ...;

			states {
				$(
					$async_mutex_cur => $async_mutex_next,
				)*
			}
		}
	)*

	$(
		rwlock::resource_impl! {
			$async_rwlock_name($async_rwlock_ty);
			fn $new(...) -> ...;

			states {
				$(
					$async_rwlock_cur => $async_rwlock_next,
				)*
			}
		}
	)*

	$(
		meetup::resource_impl! {
			$meetup_sender_name($meetup_sender_ty);
			fn $new(...) -> ...;

			states {
				$( $meetup_sender_cur, )*
			}
		}
	)*
}
