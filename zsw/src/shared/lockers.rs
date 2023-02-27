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
	zsw_util::meetup,
};

// TODO: Use custom types here, instead of these
type CurPanelGroup = Option<PanelGroup>;
type PanelsUpdaterMeetupRenderer = ();
type EguiPainterMeetupRenderer = (Vec<egui::ClippedPrimitive>, egui::TexturesDelta);

define_locker! {
	LoadDefaultPanelGroupLocker {
		inner;
		fn new(...) -> Self;

		async_mutex {
			fn lock(...) -> ...;
			cur_panel_group: CurPanelGroup = [ 0 ] => 1,
			panels_renderer_shader: PanelsRendererShader = [ 0 ] => 1,
		}

		async_rwlock {
			fn read(...) -> ...;
			fn upgradable_read(...) -> ...;
			fn write(...) -> ...;

			playlists_manager: PlaylistsManager = [ 0 ] => 1,
		}
	}

	RendererLocker {
		inner;
		fn new(...) -> Self;

		async_mutex {
			fn lock(...) -> ...;
			cur_panel_group: CurPanelGroup = [ 0 ] => 1,
			panels_renderer_shader: PanelsRendererShader = [ 0 1 ] => 2,
		}
	}

	PanelsUpdaterLocker {
		inner;
		fn new(...) -> Self;

		async_mutex {
			fn lock(...) -> ...;
			cur_panel_group: CurPanelGroup = [ 0 ] => 1,
		}

		meetup_sender {
			fn send(...) -> ...;
			panels_updater_meetup_renderer: PanelsUpdaterMeetupRenderer = [ 0 ]
		}
	}

	EguiPainterLocker {
		inner;
		fn new(...) -> Self;

		async_mutex {
			fn lock(...) -> ...;
			cur_panel_group: CurPanelGroup = [ 0 ] => 1,
			panels_renderer_shader: PanelsRendererShader = [ 0 1 ] => 2,
		}

		async_rwlock {
			fn read(...) -> ...;
			fn upgradable_read(...) -> ...;
			fn write(...) -> ...;

			playlists_manager: PlaylistsManager = [ 0 1 2 ] => 3,
		}

		meetup_sender {
			fn send(...) -> ...;
			egui_painter_meetup_renderer: EguiPainterMeetupRenderer = [ 0 ],
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
						$async_mutex_name:ident: $async_mutex_ty:ty = [ $( $async_mutex_prev:literal )* ] => $async_mutex_next:literal
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
						$async_rwlock_name:ident: $async_rwlock_ty:ty = [ $( $async_rwlock_prev:literal )* ] => $async_rwlock_next:literal
					),*
					$(,)?
				}
			)?

			$(
				meetup_sender {
					fn $send_meetup_sender:ident(...) -> ...;
					$(
						$meetup_sender_name:ident: $meetup_sender_ty:ty = [ $( $meetup_sender_prev:literal )* ]
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
					$(
						impl AsyncMutexLocker<$async_mutex_ty> for $LockerName<$async_mutex_prev> {
							type Next<'locker> = $LockerName<$async_mutex_next>;

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
				)*
			)?

			// Async rwlocks
			$(
				$(
					$(
						impl AsyncRwLockLocker<$async_rwlock_ty> for $LockerName<$async_rwlock_prev> {
							type Next<'locker> = $LockerName<$async_rwlock_next>;

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
				)*
			)?

			// Meetup senders
			$(
				$(
					$(
						impl MeetupSenderLocker<$meetup_sender_ty> for $LockerName<$meetup_sender_prev> {
							#[track_caller]
							async fn send_resource(&mut self, resource: $meetup_sender_ty) {
								#[allow(clippy::disallowed_methods)] // DEADLOCK: We ensure thread safety via the locker abstraction
								self.$inner.$meetup_sender_name.send(resource).await;
							}
						}
					)*
				)*
			)?
		)*
	}
}
