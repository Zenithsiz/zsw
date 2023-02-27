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
	std::sync::Arc,
	zsw_util::{meetup, where_assert},
};

// TODO: Use custom types here, instead of these
type AsyncMutex0 = Option<PanelGroup>;
type AsyncMutex1 = PanelsRendererShader;

type MeetupSender0 = ();
type MeetupSender1 = (Vec<egui::ClippedPrimitive>, egui::TexturesDelta);

type AsyncRwLock0 = PlaylistsManager;

define_locker! {
	LoadDefaultPanelGroupLocker {
		inner;
		fn new(...) -> Self;

		async_mutex {
			fn lock(...) -> ...;
			async_mutex0: AsyncMutex0 = 0,
			async_mutex1: AsyncMutex1 = 0,
		}

		async_rwlock {
			fn read(...) -> ...;
			fn upgradable_read(...) -> ...;
			fn write(...) -> ...;

			async_rwlock0: AsyncRwLock0 = 0,
		}
	}

	RendererLocker {
		inner;
		fn new(...) -> Self;

		async_mutex {
			fn lock(...) -> ...;
			async_mutex0: AsyncMutex0 = 0,
			async_mutex1: AsyncMutex1 = 1,
		}
	}

	PanelsUpdaterLocker {
		inner;
		fn new(...) -> Self;

		async_mutex {
			fn lock(...) -> ...;
			async_mutex0: AsyncMutex0 = 0,
		}

		meetup_sender {
			fn send(...) -> ...;
			meetup_sender0: MeetupSender0 = 0,
		}
	}

	EguiPainterLocker {
		inner;
		fn new(...) -> Self;

		async_mutex {
			fn lock(...) -> ...;
			async_mutex0: AsyncMutex0 = 0,
			async_mutex1: AsyncMutex1 = 1,
		}

		async_rwlock {
			fn read(...) -> ...;
			fn upgradable_read(...) -> ...;
			fn write(...) -> ...;

			async_rwlock0: AsyncRwLock0 = 2,
		}

		meetup_sender {
			fn send(...) -> ...;
			meetup_sender1: MeetupSender1 = 0,
		}
	}
}

macro define_locker(
	$(
		$LockerName:ident {
			$inner:ident;
			fn $new:ident(...) -> Self;

			$(
				async_mutex {
					fn $lock_async_mutex:ident(...) -> ...;
					$(
						$async_mutex_name:ident: $async_mutex_ty:ty = $async_mutex_idx:literal
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
						$async_rwlock_name:ident: $async_rwlock_ty:ty = $async_rwlock_idx:literal
					),*
					$(,)?
				}
			)?

			$(
				meetup_sender {
					fn $send_meetup_sender:ident(...) -> ...;
					$(
						$meetup_sender_name:ident: $meetup_sender_ty:ty = $meetup_sender_idx:literal
					),*
					$(,)?
				}
			)?
		}
	)*

) {
	paste::paste! {
		$(
			/// Locker inner
			#[derive(Debug)]
			pub struct [< $LockerName Inner >] {
				// Async mutexes
				$(
					$(
						$async_mutex_name: Arc<Mutex<$async_mutex_ty>>,
					)*
				)?

				// Async rwlocks
				$(
					$(
						$async_rwlock_name: Arc<RwLock<$async_rwlock_ty>>,
					)*
				)?

				// Meetup sender
				$(
					$(
						$meetup_sender_name: meetup::Sender<$meetup_sender_ty>,
					)*
				)?
			}

			/// Locker
			///
			/// # Thread safety
			/// Multiple lockers should not be created per task
			#[derive(Debug)]
			pub struct $LockerName<const STATE: usize = 0> {
				$inner: &'static [< $LockerName Inner >],
			}

			impl<const STATE: usize> $LockerName<STATE> {
				/// Creates a new locker
				pub fn $new(
					// Async mutexes
					$( $( $async_mutex_name: Arc<Mutex<$async_mutex_ty>>, )* )?

					// Async rwlocks
					$( $( $async_rwlock_name: Arc<RwLock<$async_rwlock_ty>>, )* )?

					// Meetup senders
					$( $( $meetup_sender_name: meetup::Sender<$meetup_sender_ty>, )* )?
				) -> Self {
					// TODO: Don't leak and instead drop it only when dropping when `STATE == 0`.
					let inner = [< $LockerName Inner >] {
						// Async mutexes
						$( $( $async_mutex_name, )* )?

						// Async rwlocks
						$( $( $async_rwlock_name, )* )?

						// Meetup senders
						$( $( $meetup_sender_name, )* )?
					};
					let inner = Box::leak(Box::new(inner));

					Self { $inner: inner }
				}

				$(
					/// Locks the async mutex `R`
					#[track_caller]
					pub async fn $lock_async_mutex<'locker, R>(
						&'locker mut self,
					) -> (MutexGuard<'locker, R>, <Self as AsyncMutexLocker<R>>::Next<'locker>)
					where
						Self: AsyncMutexLocker<R>,
						R: 'locker,
					{
						self.lock_resource().await
					}
				)?

				$(
					/// Locks the async rwlock `R` for reading
					#[track_caller]
					pub async fn $lock_async_rwlock_read<'locker, R>(
						&'locker mut self,
					) -> (RwLockReadGuard<'locker, R>, <Self as AsyncRwLockLocker<R>>::Next<'locker>)
					where
						Self: AsyncRwLockLocker<R>,
						R: 'locker,
					{
						self.lock_read_resource().await
					}

					/// Locks the async rwlock `R` for an upgradable reading
					#[track_caller]
					pub async fn $lock_async_rwlock_upgradable_read<'locker, R>(
						&'locker mut self,
					) -> (RwLockUpgradableReadGuard<'locker, R>, <Self as AsyncRwLockLocker<R>>::Next<'locker>)
					where
						Self: AsyncRwLockLocker<R>,
						R: 'locker,
					{
						self.lock_upgradable_read_resource().await
					}

					/// Locks the async rwlock `R` for writing
					#[track_caller]
					pub async fn $lock_async_rwlock_write<'locker, R>(
						&'locker mut self,
					) -> (RwLockWriteGuard<'locker, R>, <Self as AsyncRwLockLocker<R>>::Next<'locker>)
					where
						Self: AsyncRwLockLocker<R>,
						R: 'locker,
					{
						self.lock_write_resource().await
					}
				)?

				$(
					/// Sends the resource `R` to it's meetup channel
					#[track_caller]
					pub async fn $send_meetup_sender<R>(
						&mut self,
						resource: R,
					)
					where
						Self: MeetupSenderLocker<R>
					{
						self.send_resource(resource).await;
					}
				)?
			}

			// Async mutexes
			$(
				$(
					impl<const CUR_STATE: usize> AsyncMutexLocker<$async_mutex_ty> for $LockerName<CUR_STATE>
					where
						// Note: This means that any state up to (including) `$async_mutex_idx` can lock the resource.
						//       The returned locker will always be at state `$async_mutex_idx + 1`, regardless of where
						//       it was called from. This ensures that we can never lock mutexes out of order.
						where_assert!(CUR_STATE <= $async_mutex_idx):,
					{
						type Next<'locker> = $LockerName<{ $async_mutex_idx + 1 }>;

						#[track_caller]
						async fn lock_resource<'locker>(&'locker mut self) -> (MutexGuard<$async_mutex_ty>, Self::Next<'locker>)
						where
							$async_mutex_ty: 'locker,
						{
							#[allow(clippy::disallowed_methods)] // DEADLOCK: We ensure thread safety via the locker abstraction
							let guard = self.$inner.$async_mutex_name.lock().await;
							let locker = $LockerName {
								$inner: self.$inner
							};
							(guard, locker)
						}
					}
				)*
			)?

			// Async rwlocks
			$(
				$(
					impl<const CUR_STATE: usize> AsyncRwLockLocker<$async_rwlock_ty> for $LockerName<CUR_STATE>
					where
						// Note: See the note on the similar state in the mutexes section
						where_assert!(CUR_STATE <= $async_rwlock_idx):,
					{
						type Next<'locker> = $LockerName<{ $async_rwlock_idx + 1 }>;

						#[track_caller]
						async fn lock_read_resource<'locker>(&'locker mut self) -> (RwLockReadGuard<$async_rwlock_ty>, Self::Next<'locker>)
						where
							$async_rwlock_ty: 'locker
						{
							#[allow(clippy::disallowed_methods)] // DEADLOCK: We ensure thread safety via the locker abstraction
							let guard = self.$inner.$async_rwlock_name.read().await;
							let locker = $LockerName {
								$inner: self.$inner
							};
							(guard, locker)
						}

						#[track_caller]
						async fn lock_upgradable_read_resource<'locker>(
							&'locker mut self,
						) -> (RwLockUpgradableReadGuard<$async_rwlock_ty>, Self::Next<'locker>)
						where
							$async_rwlock_ty: 'locker
							{
								#[allow(clippy::disallowed_methods)] // DEADLOCK: We ensure thread safety via the locker abstraction
								let guard = self.$inner.$async_rwlock_name.upgradable_read().await;
								let locker = $LockerName {
									$inner: self.$inner
								};
								(guard, locker)
							}

						#[track_caller]
						async fn lock_write_resource<'locker>(&'locker mut self) -> (RwLockWriteGuard<$async_rwlock_ty>, Self::Next<'locker>)
						where
							$async_rwlock_ty: 'locker
							{
								#[allow(clippy::disallowed_methods)] // DEADLOCK: We ensure thread safety via the locker abstraction
								let guard = self.$inner.$async_rwlock_name.write().await;
								let locker = $LockerName {
									$inner: self.$inner
								};
								(guard, locker)
							}
					}
				)*
			)?

			// Meetup senders
			$(
				$(
					impl<const CUR_STATE: usize> MeetupSenderLocker<$meetup_sender_ty> for $LockerName<CUR_STATE>
					where
						// Note: This means that any state up to (including) `$async_mutex_idx` can send the resource.
						//       Unlike async mutexes, we only care that a certain mutex isn't locked when this is called,
						//       so we don't need to return any next locker.
						where_assert!(CUR_STATE <= $meetup_sender_idx):,
					{
						#[track_caller]
						async fn send_resource(&mut self, resource: $meetup_sender_ty) {
							#[allow(clippy::disallowed_methods)] // DEADLOCK: We ensure thread safety via the locker abstraction
							self.$inner.$meetup_sender_name.send(resource).await;
						}
					}
				)*
			)?
		)*
	}
}
