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
