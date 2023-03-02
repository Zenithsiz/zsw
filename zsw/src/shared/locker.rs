//! Locker

// Imports
use {
	crate::{
		panel::{PanelGroup, PanelsRendererShader},
		playlist::Playlists,
	},
	tokio::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard},
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

/// Locker of async mutex `R`
pub trait AsyncMutexLocker<R> {
	/// Next locker
	type Next<'locker>
	where
		Self: 'locker;

	/// Locks the resource `R` and returns the next locker
	async fn lock_resource<'locker, 'mutex>(
		&'locker mut self,
		mutex: &'mutex Mutex<R>,
	) -> (MutexGuard<'mutex, R>, Self::Next<'locker>)
	where
		R: 'locker;

	/// Blockingly locks the resource `R` and returns the next locker
	fn blocking_lock_resource<'locker, 'mutex>(
		&'locker mut self,
		mutex: &'mutex Mutex<R>,
	) -> (MutexGuard<'mutex, R>, Self::Next<'locker>);
}

/// Locker of async rwlock `R`
pub trait AsyncRwLockLocker<R> {
	/// Next locker
	type Next<'locker>
	where
		Self: 'locker;

	/// Locks the resource `R` for read and returns the next locker
	async fn lock_read_resource<'locker, 'rwlock>(
		&'locker mut self,
		rwlock: &'rwlock RwLock<R>,
	) -> (RwLockReadGuard<'rwlock, R>, Self::Next<'locker>)
	where
		R: 'locker;

	/// Blockingly locks the resource `R` for read and returns the next locker
	fn blocking_lock_read_resource<'locker, 'rwlock>(
		&'locker mut self,
		rwlock: &'rwlock RwLock<R>,
	) -> (RwLockReadGuard<'rwlock, R>, Self::Next<'locker>);

	/// Locks the resource `R` for write and returns the next locker
	async fn lock_write_resource<'locker, 'rwlock>(
		&'locker mut self,
		rwlock: &'rwlock RwLock<R>,
	) -> (RwLockWriteGuard<'rwlock, R>, Self::Next<'locker>)
	where
		R: 'locker;

	/// Blockingly locks the resource `R` for write and returns the next locker
	fn blocking_lock_write_resource<'locker, 'rwlock>(
		&'locker mut self,
		rwlock: &'rwlock RwLock<R>,
	) -> (RwLockWriteGuard<'rwlock, R>, Self::Next<'locker>);
}

/// Locker of meetup sender of `R`
pub trait MeetupSenderLocker<R> {
	/// Sends the resource `R` to the meetup channel
	async fn send_resource(&mut self, tx: &meetup::Sender<R>, resource: R);

	// TODO: Blocking version?
}

// TODO: Separate into several extension traits for each locker type?
#[extend::ext(name = LockerExt)]
pub impl<L> L {
	/// Locks the async mutex `R`
	#[track_caller]
	async fn mutex_lock<'locker, 'mutex, R>(
		&'locker mut self,
		mutex: &'mutex Mutex<R>,
	) -> (MutexGuard<'mutex, R>, <Self as AsyncMutexLocker<R>>::Next<'locker>)
	where
		Self: AsyncMutexLocker<R>,
		R: 'locker,
	{
		self.lock_resource(mutex).await
	}

	/// Blockingly locks the async mutex `R`
	#[track_caller]
	fn blocking_mutex_lock<'locker, 'mutex, R>(
		&'locker mut self,
		mutex: &'mutex Mutex<R>,
	) -> (MutexGuard<'mutex, R>, <Self as AsyncMutexLocker<R>>::Next<'locker>)
	where
		Self: AsyncMutexLocker<R>,
		R: 'locker,
	{
		self.blocking_lock_resource(mutex)
	}

	/// Locks the async rwlock `R` for reading
	#[track_caller]
	async fn rwlock_read<'locker, 'rwlock, R>(
		&'locker mut self,
		rwlock: &'rwlock RwLock<R>,
	) -> (
		RwLockReadGuard<'rwlock, R>,
		<Self as AsyncRwLockLocker<R>>::Next<'locker>,
	)
	where
		Self: AsyncRwLockLocker<R>,
		R: 'locker,
	{
		self.lock_read_resource(rwlock).await
	}

	/// Blockingly locks the async rwlock `R` for reading
	#[track_caller]
	fn blocking_rwlock_read<'locker, 'rwlock, R>(
		&'locker mut self,
		rwlock: &'rwlock RwLock<R>,
	) -> (
		RwLockReadGuard<'rwlock, R>,
		<Self as AsyncRwLockLocker<R>>::Next<'locker>,
	)
	where
		Self: AsyncRwLockLocker<R>,
		R: 'locker,
	{
		self.blocking_lock_read_resource(rwlock)
	}

	/// Locks the async rwlock `R` for writing
	#[track_caller]
	async fn rwlock_write<'locker, 'rwlock, R>(
		&'locker mut self,
		rwlock: &'rwlock RwLock<R>,
	) -> (
		RwLockWriteGuard<'rwlock, R>,
		<Self as AsyncRwLockLocker<R>>::Next<'locker>,
	)
	where
		Self: AsyncRwLockLocker<R>,
		R: 'locker,
	{
		self.lock_write_resource(rwlock).await
	}

	/// Blockingly locks the async rwlock `R` for writing
	#[track_caller]
	fn blocking_rwlock_write<'locker, 'rwlock, R>(
		&'locker mut self,
		rwlock: &'rwlock RwLock<R>,
	) -> (
		RwLockWriteGuard<'rwlock, R>,
		<Self as AsyncRwLockLocker<R>>::Next<'locker>,
	)
	where
		Self: AsyncRwLockLocker<R>,
		R: 'locker,
	{
		self.blocking_lock_write_resource(rwlock)
	}

	/// Sends the resource `R` to it's meetup channel
	#[track_caller]
	async fn meetup_send<R>(&mut self, tx: &meetup::Sender<R>, resource: R)
	where
		Self: MeetupSenderLocker<R>,
	{
		self.send_resource(tx, resource).await;
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

				#[track_caller]
				fn blocking_lock_resource<'locker, 'mutex>(
					&'locker mut self,
					mutex: &'mutex Mutex<$async_mutex_ty>
				) -> (MutexGuard<'mutex, $async_mutex_ty>, Self::Next<'locker>)
				{
					#[allow(clippy::disallowed_methods)] // DEADLOCK: We ensure thread safety via the locker abstraction
					let guard = tokio::task::block_in_place(|| mutex.blocking_lock());
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
				fn blocking_lock_read_resource<'locker, 'rwlock>(
					&'locker mut self,
					rwlock: &'rwlock RwLock<$async_rwlock_ty>
				) -> (RwLockReadGuard<'rwlock, $async_rwlock_ty>, Self::Next<'locker>)
				{
					#[allow(clippy::disallowed_methods)] // DEADLOCK: We ensure thread safety via the locker abstraction
					let guard = tokio::task::block_in_place(|| rwlock.blocking_read());
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

				#[track_caller]
				fn blocking_lock_write_resource<'locker, 'rwlock>(
					&'locker mut self,
					rwlock: &'rwlock RwLock<$async_rwlock_ty>
				) -> (RwLockWriteGuard<'rwlock, $async_rwlock_ty>, Self::Next<'locker>)
				{
					#[allow(clippy::disallowed_methods)] // DEADLOCK: We ensure thread safety via the locker abstraction
					let guard = tokio::task::block_in_place(|| rwlock.blocking_write());
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
