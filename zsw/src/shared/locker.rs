//! Locker

// Imports
use {
	async_lock::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockUpgradableReadGuard, RwLockWriteGuard},
	zsw_util::meetup,
};

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

	/// Locks the resource `R` for upgradable read and returns the next locker
	async fn lock_upgradable_read_resource<'locker, 'rwlock>(
		&'locker mut self,
		rwlock: &'rwlock RwLock<R>,
	) -> (RwLockUpgradableReadGuard<'rwlock, R>, Self::Next<'locker>)
	where
		R: 'locker;

	/// Locks the resource `R` for write and returns the next locker
	async fn lock_write_resource<'locker, 'rwlock>(
		&'locker mut self,
		rwlock: &'rwlock RwLock<R>,
	) -> (RwLockWriteGuard<'rwlock, R>, Self::Next<'locker>)
	where
		R: 'locker;
}

/// Locker of meetup sender of `R`
pub trait MeetupSenderLocker<R> {
	/// Sends the resource `R` to the meetup channel
	async fn send_resource(&mut self, tx: &meetup::Sender<R>, resource: R);
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
		tokio::runtime::Handle::current().block_on(self.mutex_lock(mutex))
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
		tokio::runtime::Handle::current().block_on(self.rwlock_read(rwlock))
	}

	/// Locks the async rwlock `R` for an upgradable reading
	#[track_caller]
	async fn rwlock_upgradable_read<'locker, 'rwlock, R>(
		&'locker mut self,
		rwlock: &'rwlock RwLock<R>,
	) -> (
		RwLockUpgradableReadGuard<'rwlock, R>,
		<Self as AsyncRwLockLocker<R>>::Next<'locker>,
	)
	where
		Self: AsyncRwLockLocker<R>,
		R: 'locker,
	{
		self.lock_upgradable_read_resource(rwlock).await
	}

	/// Blockingly locks the async rwlock `R` for an upgradable reading
	#[track_caller]
	fn blocking_rwlock_upgradable_read<'locker, 'rwlock, R>(
		&'locker mut self,
		rwlock: &'rwlock RwLock<R>,
	) -> (
		RwLockUpgradableReadGuard<'rwlock, R>,
		<Self as AsyncRwLockLocker<R>>::Next<'locker>,
	)
	where
		Self: AsyncRwLockLocker<R>,
		R: 'locker,
	{
		tokio::runtime::Handle::current().block_on(self.rwlock_upgradable_read(rwlock))
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
	async fn blocking_rwlock_write<'locker, 'rwlock, R>(
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
		tokio::runtime::Handle::current().block_on(self.rwlock_write(rwlock))
	}

	/// Sends the resource `R` to it's meetup channel
	#[track_caller]
	async fn meetup_send<R>(&mut self, tx: &meetup::Sender<R>, resource: R)
	where
		Self: MeetupSenderLocker<R>,
	{
		self.send_resource(tx, resource).await;
	}

	/// Blockingly sends the resource `R` to it's meetup channel
	#[track_caller]
	async fn blocking_meetup_send<R>(&mut self, tx: &meetup::Sender<R>, resource: R)
	where
		Self: MeetupSenderLocker<R>,
	{
		tokio::runtime::Handle::current().block_on(self.meetup_send(tx, resource));
	}
}
