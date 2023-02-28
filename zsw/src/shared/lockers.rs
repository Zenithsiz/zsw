//! Lockers

// Imports
use {
	super::{AsyncMutexLocker, AsyncRwLockLocker, MeetupSenderLocker},
	crate::{
		panel::{PanelGroup, PanelsRendererShader},
		playlist::PlaylistsManager,
	},
	async_lock::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockUpgradableReadGuard, RwLockWriteGuard},
	std::sync::atomic::{self, AtomicBool},
	zsw_util::meetup,
};

// TODO: Use custom types here, instead of these
type CurPanelGroup = Option<PanelGroup>;
type PanelsUpdaterMeetupRenderer = ();
type EguiPainterMeetupRenderer = (Vec<egui::ClippedPrimitive>, egui::TexturesDelta);

define_locker! {
	fn new(...) -> Self;

	fn lock(...) -> ...;
	fn blocking_lock(...) -> ...;

	fn read(...) -> ...;
	fn upgradable_read(...) -> ...;
	fn write(...) -> ...;
	fn blocking_read(...) -> ...;
	fn blocking_upgradable_read(...) -> ...;
	fn blocking_write(...) -> ...;

	fn send(...) -> ...;
	fn blocking_send(...) -> ...;

	LoadDefaultPanelGroupLocker {
		async_mutex {
			CurPanelGroup = [ 0 ] => 1,
			PanelsRendererShader = [ 0 ] => 1,
		}

		async_rwlock {
			PlaylistsManager = [ 0 ] => 1,
		}
	}

	RendererLocker {
		async_mutex {
			CurPanelGroup = [ 0 ] => 1,
			PanelsRendererShader = [ 0 1 ] => 2,
		}
	}

	PanelsUpdaterLocker {
		async_mutex {
			CurPanelGroup = [ 0 ] => 1,
		}

		meetup_sender {
			PanelsUpdaterMeetupRenderer = [ 0 ]
		}
	}

	EguiPainterLocker {
		async_mutex {
			CurPanelGroup = [ 0 ] => 1,
			PanelsRendererShader = [ 0 1 ] => 2,
		}

		async_rwlock {
			PlaylistsManager = [ 0 1 2 ] => 3,
		}

		meetup_sender {
			EguiPainterMeetupRenderer = [ 0 ],
		}
	}
}

macro define_locker(
	fn $new:ident(...) -> Self;

	// Mutex
	fn $lock_async_mutex:ident(...) -> ...;
	fn $blocking_lock_async_mutex:ident(...) -> ...;

	// RwLock
	fn $lock_async_rwlock_read:ident(...) -> ...;
	fn $lock_async_rwlock_upgradable_read:ident(...) -> ...;
	fn $lock_async_rwlock_write:ident(...) -> ...;
	fn $blocking_lock_async_rwlock_read:ident(...) -> ...;
	fn $blocking_lock_async_rwlock_upgradable_read:ident(...) -> ...;
	fn $blocking_lock_async_rwlock_write:ident(...) -> ...;

	// meetup::Sender
	fn $send_meetup_sender:ident(...) -> ...;
	fn $blocking_send_meetup_sender:ident(...) -> ...;

	$(
		$LockerName:ident {

			$(
				async_mutex {
					$(
						$async_mutex_ty:ty = [ $( $async_mutex_prev:literal )* ] => $async_mutex_next:literal
					),*
					$(,)?
				}
			)?

			$(
				async_rwlock {
					$(
						$async_rwlock_ty:ty = [ $( $async_rwlock_prev:literal )* ] => $async_rwlock_next:literal
					),*
					$(,)?
				}
			)?

			$(
				meetup_sender {
					$(
						$meetup_sender_ty:ty = [ $( $meetup_sender_prev:literal )* ]
					),*
					$(,)?
				}
			)?
		}
	)*

) {
	paste::paste! {
		$(
			/// Locker
			///
			/// # Thread safety
			/// Multiple lockers should not be created per task
			#[derive(Debug)]
			pub struct $LockerName<const STATE: usize = 0>(());

			impl $LockerName<0> {
				/// Creates a new locker.
				///
				/// Panics if called more than once
				// TODO: Allow escape hatch for when we need to re-boot a task if it panics or something?
				//       In this case, we'd likely associate it with a task-id so we can make sure that the
				//       previous task never re-locks again or similar?
				pub fn $new() -> Self {
					static CREATED: AtomicBool = AtomicBool::new(false);
					assert!(
						!CREATED.swap(true, atomic::Ordering::AcqRel),
						"Cannot create locker twice"
					);

					Self(())
				}
			}

			// Async mutexes
			$(
				$(
					$(
						impl AsyncMutexLocker<$async_mutex_ty> for $LockerName<$async_mutex_prev> {
							type Next<'locker> = $LockerName<$async_mutex_next>;

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
								let locker = $LockerName(());
								(guard, locker)
							}
						}
					)*
				)*
			)?

			// Async rwlocks
			$(
				$(
					$(
						impl AsyncRwLockLocker<$async_rwlock_ty> for $LockerName<$async_rwlock_prev> {
							type Next<'locker> = $LockerName<$async_rwlock_next>;

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
								let locker = $LockerName(());
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
									let locker = $LockerName(());
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
									let locker = $LockerName(());
									(guard, locker)
								}
						}
					)*
				)*
			)?

			// Meetup senders
			$(
				$(
					$(
						impl MeetupSenderLocker<$meetup_sender_ty> for $LockerName<$meetup_sender_prev> {
							#[track_caller]
							async fn send_resource(&mut self, tx: &meetup::Sender<$meetup_sender_ty>, resource: $meetup_sender_ty) {
								#[allow(clippy::disallowed_methods)] // DEADLOCK: We ensure thread safety via the locker abstraction
								tx.send(resource).await;
							}
						}
					)*
				)*
			)?
		)*
	}
}
