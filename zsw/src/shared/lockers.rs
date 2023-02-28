//! Lockers

// TODO: Use more descriptive names than `lock` and `send`?

// Lints
#![allow(dead_code)] // We define a very generic macro that may expand to functions that aren't needed

// Imports
use {
	super::locker::{AsyncMutexLocker, AsyncRwLockLocker, MeetupSenderLocker},
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
	LoadDefaultPanelGroupLocker {
		fn new(...) -> Self;

		async_mutex {
			fn lock(...) -> ...;
			CurPanelGroup = [ 0 ] => 1,
			PanelsRendererShader = [ 0 ] => 1,
		}

		async_rwlock {
			fn read(...) -> ...;
			fn upgradable_read(...) -> ...;
			fn write(...) -> ...;

			PlaylistsManager = [ 0 ] => 1,
		}
	}

	RendererLocker {
		fn new(...) -> Self;

		async_mutex {
			fn lock(...) -> ...;
			CurPanelGroup = [ 0 ] => 1,
			PanelsRendererShader = [ 0 1 ] => 2,
		}
	}

	PanelsUpdaterLocker {
		fn new(...) -> Self;

		async_mutex {
			fn lock(...) -> ...;
			CurPanelGroup = [ 0 ] => 1,
		}

		meetup_sender {
			fn send(...) -> ...;
			PanelsUpdaterMeetupRenderer = [ 0 ]
		}
	}

	EguiPainterLocker {
		fn new(...) -> Self;

		async_mutex {
			fn lock(...) -> ...;
			CurPanelGroup = [ 0 ] => 1,
			PanelsRendererShader = [ 0 1 ] => 2,
		}

		async_rwlock {
			fn read(...) -> ...;
			fn upgradable_read(...) -> ...;
			fn write(...) -> ...;

			PlaylistsManager = [ 0 1 2 ] => 3,
		}

		meetup_sender {
			fn send(...) -> ...;
			EguiPainterMeetupRenderer = [ 0 ],
		}
	}
}

macro define_locker(
	$(
		$LockerName:ident {
			fn $new:ident(...) -> Self;

			$(
				async_mutex {
					fn $lock_async_mutex:ident(...) -> ...;
					$(
						$async_mutex_ty:ty = [ $( $async_mutex_prev:literal )* ] => $async_mutex_next:literal
					),*
					$(,)?
				}
			)?

			$(
				async_rwlock {
					fn $lock_async_rwlock_read:ident(...) -> ...;
					fn $lock_async_rwlock_upgradable_read:ident(...) -> ...;
					fn $lock_async_rwlock_write:ident(...) -> ...;
					$(
						$async_rwlock_ty:ty = [ $( $async_rwlock_prev:literal )* ] => $async_rwlock_next:literal
					),*
					$(,)?
				}
			)?

			$(
				meetup_sender {
					fn $send_meetup_sender:ident(...) -> ...;
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

			impl<const STATE: usize> $LockerName<STATE> {
				/// Creates a new locker.
				///
				/// Panics if called more than once
				// TODO: Allow escape hatch for when we need to re-boot a task if it panics or something?
				//       In this case, we'd likely associate it with a task-id so we can make sure that the
				//       previous task never re-locks again or similar?
				pub fn $new() -> Self {
					static IS_CREATED: AtomicBool = AtomicBool::new(false);
					assert!(
						!IS_CREATED.swap(true, atomic::Ordering::AcqRel),
						"Cannot create locker twice"
					);

					Self(())
				}

				$(
					/// Locks the async mutex `R`
					#[track_caller]
					pub async fn $lock_async_mutex<'locker, 'mutex, R>(
						&'locker mut self,
						mutex: &'mutex Mutex<R>,
					) -> (MutexGuard<'mutex, R>, <Self as AsyncMutexLocker<R>>::Next<'locker>)
					where
						Self: AsyncMutexLocker<R>,
						R: 'locker,
					{
						self.lock_resource(mutex).await
					}
				)?

				$(
					/// Locks the async rwlock `R` for reading
					#[track_caller]
					pub async fn $lock_async_rwlock_read<'locker, 'rwlock, R>(
						&'locker mut self,
						rwlock: &'rwlock RwLock<R>,
					) -> (RwLockReadGuard<'rwlock, R>, <Self as AsyncRwLockLocker<R>>::Next<'locker>)
					where
						Self: AsyncRwLockLocker<R>,
						R: 'locker,
					{
						self.lock_read_resource(rwlock).await
					}

					/// Locks the async rwlock `R` for an upgradable reading
					#[track_caller]
					pub async fn $lock_async_rwlock_upgradable_read<'locker, 'rwlock, R>(
						&'locker mut self,
						rwlock: &'rwlock RwLock<R>,
					) -> (RwLockUpgradableReadGuard<'rwlock, R>, <Self as AsyncRwLockLocker<R>>::Next<'locker>)
					where
						Self: AsyncRwLockLocker<R>,
						R: 'locker,
					{
						self.lock_upgradable_read_resource(rwlock).await
					}

					/// Locks the async rwlock `R` for writing
					#[track_caller]
					pub async fn $lock_async_rwlock_write<'locker, 'rwlock, R>(
						&'locker mut self,
						rwlock: &'rwlock RwLock<R>,
					) -> (RwLockWriteGuard<'rwlock, R>, <Self as AsyncRwLockLocker<R>>::Next<'locker>)
					where
						Self: AsyncRwLockLocker<R>,
						R: 'locker,
					{
						self.lock_write_resource(rwlock).await
					}
				)?

				$(
					/// Sends the resource `R` to it's meetup channel
					#[track_caller]
					pub async fn $send_meetup_sender<R>(
						&mut self,
						tx: &meetup::Sender<R>,
						resource: R,
					)
					where
						Self: MeetupSenderLocker<R>
					{
						self.send_resource(tx, resource).await;
					}
				)?
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
