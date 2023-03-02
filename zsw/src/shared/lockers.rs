//! Lockers

// TODO: Move this to the `locker` module.

// Imports
use {
	super::{AsyncMutexLocker, AsyncRwLockLocker, MeetupSenderLocker},
	crate::{
		panel::{PanelGroup, PanelsRendererShader},
		playlist::Playlists,
	},
	async_lock::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockUpgradableReadGuard, RwLockWriteGuard},
	zsw_util::meetup,
};

// TODO: Use custom types here, instead of these
type CurPanelGroup = Option<PanelGroup>;
type PanelsUpdaterMeetupRenderer = ();
type EguiPainterMeetupRenderer = (Vec<egui::ClippedPrimitive>, egui::TexturesDelta);

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
	async_mutex {
		CurPanelGroup = [ 0 ] => 1,
	}

	async_rwlock {
		Playlists = [ 0 ] => 1,
		PanelsRendererShader = [ 0 1 ] => 2,
	}

	meetup_sender {
		PanelsUpdaterMeetupRenderer = [ 0 ],
		EguiPainterMeetupRenderer = [ 0 ],
	}
}

macro locker_impls(
	async_mutex {
		$(
			$async_mutex_ty:ty = [ $( $async_mutex_prev:literal )* ] => $async_mutex_next:literal
		),*
		$(,)?
	}

	async_rwlock {
		$(
			$async_rwlock_ty:ty = [ $( $async_rwlock_prev:literal )* ] => $async_rwlock_next:literal
		),*
		$(,)?
	}

	meetup_sender {
		$(
			$meetup_sender_ty:ty = [ $( $meetup_sender_prev:literal )* ]
		),*
		$(,)?
	}
) {
	// Async mutexes
	$(
		$(
			impl AsyncMutexLocker<$async_mutex_ty> for Locker<$async_mutex_prev> {
				type Next<'locker> = Locker<$async_mutex_next>;

				#[track_caller]
				async fn lock_resource<'locker, 'mutex>(
					&'locker mut self,
					mutex: &'mutex Mutex<$async_mutex_ty>
				) -> (MutexGuard<'mutex, $async_mutex_ty>, Self::Next<'locker>)
				where
					$async_mutex_ty: 'locker,
				{
					#[allow(clippy::disallowed_methods)] // DEADLOCK: We ensure thread safety via the locker abstraction
					let guard = mutex.lock().await;
					let locker = Locker(());
					(guard, locker)
				}
			}
		)*
	)*

	// Async rwlocks
	$(
		$(
			impl AsyncRwLockLocker<$async_rwlock_ty> for Locker<$async_rwlock_prev> {
				type Next<'locker> = Locker<$async_rwlock_next>;

				#[track_caller]
				async fn lock_read_resource<'locker, 'rwlock>(
					&'locker mut self,
					rwlock: &'rwlock RwLock<$async_rwlock_ty>
				) -> (RwLockReadGuard<'rwlock, $async_rwlock_ty>, Self::Next<'locker>)
				where
					$async_rwlock_ty: 'locker
				{
					#[allow(clippy::disallowed_methods)] // DEADLOCK: We ensure thread safety via the locker abstraction
					let guard = rwlock.read().await;
					let locker = Locker(());
					(guard, locker)
				}

				#[track_caller]
				async fn lock_upgradable_read_resource<'locker, 'rwlock>(
					&'locker mut self,
					rwlock: &'rwlock RwLock<$async_rwlock_ty>
				) -> (RwLockUpgradableReadGuard<'rwlock, $async_rwlock_ty>, Self::Next<'locker>)
				where
					$async_rwlock_ty: 'locker
					{
						#[allow(clippy::disallowed_methods)] // DEADLOCK: We ensure thread safety via the locker abstraction
						let guard = rwlock.upgradable_read().await;
						let locker = Locker(());
						(guard, locker)
					}

				#[track_caller]
				async fn lock_write_resource<'locker, 'rwlock>(
					&'locker mut self,
					rwlock: &'rwlock RwLock<$async_rwlock_ty>
				) -> (RwLockWriteGuard<'rwlock, $async_rwlock_ty>, Self::Next<'locker>)
				where
					$async_rwlock_ty: 'locker
					{
						#[allow(clippy::disallowed_methods)] // DEADLOCK: We ensure thread safety via the locker abstraction
						let guard = rwlock.write().await;
						let locker = Locker(());
						(guard, locker)
					}
			}
		)*
	)*

	// Meetup senders
	$(
		$(
			impl MeetupSenderLocker<$meetup_sender_ty> for Locker<$meetup_sender_prev> {
				#[track_caller]
				async fn send_resource(&mut self, tx: &meetup::Sender<$meetup_sender_ty>, resource: $meetup_sender_ty) {
					#[allow(clippy::disallowed_methods)] // DEADLOCK: We ensure thread safety via the locker abstraction
					tx.send(resource).await;
				}
			}
		)*
	)*
}
