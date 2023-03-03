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

// Lock implementations
#[expect(
	clippy::unused_self,
	reason = "The locker doesn't actually do anything aside from acting as an abstraction"
)]
#[expect(
	clippy::disallowed_methods,
	reason = "DEADLOCK: We ensure thread safety via the locker abstraction"
)]
impl<const STATE: usize> Locker<STATE> {
	/// Locks the async mutex `R`
	#[track_caller]
	pub async fn mutex_lock<'locker, R>(
		&'locker mut self,
		mutex: &'locker Mutex<R>,
	) -> (MutexGuard<'locker, R>, Locker<{ Self::NEXT_STATE }>)
	where
		Self: AsyncMutexLocker<R>,
		[(); Self::NEXT_STATE]:,
	{
		let guard = mutex.lock().await;
		let locker = Locker(());
		(guard, locker)
	}

	/// Blockingly locks the async mutex `R`
	#[track_caller]
	pub fn blocking_mutex_lock<'locker, R>(
		&'locker mut self,
		mutex: &'locker Mutex<R>,
	) -> (MutexGuard<'locker, R>, Locker<{ Self::NEXT_STATE }>)
	where
		Self: AsyncMutexLocker<R>,
		[(); Self::NEXT_STATE]:,
	{
		let guard = mutex.blocking_lock();
		let locker = Locker(());
		(guard, locker)
	}

	/// Locks the async rwlock `R` for reading
	#[track_caller]
	pub async fn rwlock_read<'locker, R>(
		&'locker mut self,
		rwlock: &'locker RwLock<R>,
	) -> (RwLockReadGuard<'locker, R>, Locker<{ Self::NEXT_STATE }>)
	where
		Self: AsyncRwLockLocker<R>,
		[(); Self::NEXT_STATE]:,
	{
		let guard = rwlock.read().await;
		let locker = Locker(());
		(guard, locker)
	}

	/// Blockingly locks the async rwlock `R` for reading
	#[track_caller]
	pub fn _blocking_rwlock_read<'locker, R>(
		&'locker mut self,
		rwlock: &'locker RwLock<R>,
	) -> (RwLockReadGuard<'locker, R>, Locker<{ Self::NEXT_STATE }>)
	where
		Self: AsyncRwLockLocker<R>,
		[(); Self::NEXT_STATE]:,
	{
		let guard = rwlock.blocking_read();
		let locker = Locker(());
		(guard, locker)
	}

	/// Locks the async rwlock `R` for writing
	#[track_caller]
	pub async fn rwlock_write<'locker, R>(
		&'locker mut self,
		rwlock: &'locker RwLock<R>,
	) -> (RwLockWriteGuard<'locker, R>, Locker<{ Self::NEXT_STATE }>)
	where
		Self: AsyncRwLockLocker<R>,
		[(); Self::NEXT_STATE]:,
	{
		let guard = rwlock.write().await;
		let locker = Locker(());
		(guard, locker)
	}

	/// Blockingly locks the async rwlock `R` for writing
	#[track_caller]
	pub fn blocking_rwlock_write<'locker, R>(
		&'locker mut self,
		rwlock: &'locker RwLock<R>,
	) -> (RwLockWriteGuard<'locker, R>, Locker<{ Self::NEXT_STATE }>)
	where
		Self: AsyncRwLockLocker<R>,
		[(); Self::NEXT_STATE]:,
	{
		let guard = rwlock.blocking_write();
		let locker = Locker(());
		(guard, locker)
	}

	/// Sends the resource `R` to it's meetup channel
	#[track_caller]
	pub async fn meetup_send<R>(&mut self, tx: &meetup::Sender<R>, resource: R)
	where
		Self: MeetupSenderLocker<R>,
	{
		tx.send(resource).await;
	}
}

mod sealed {
	/// Locker for `tokio::sync::Mutex<R>`
	pub trait AsyncMutexLocker<R> {
		const NEXT_STATE: usize;
	}

	/// Locker for `tokio::sync::RwLock<R>`
	pub trait AsyncRwLockLocker<R> {
		const NEXT_STATE: usize;
	}

	/// Locker for `zsw_util::meetup::Sender<R>`
	// Note: No `NEXT_STATE`, as we don't keep anything locked.
	pub trait MeetupSenderLocker<R> {}
}
#[allow(clippy::wildcard_imports)] // It just contains the sealed traits
use sealed::*;


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
		$( $async_mutex_ty:ty = [ $( $async_mutex_cur:literal )* ] => $async_mutex_next:literal ),* $(,)?
	}

	async_rwlock {
		$( $async_rwlock_ty:ty = [ $( $async_rwlock_cur:literal )* ] => $async_rwlock_next:literal ),* $(,)?
	}

	meetup_sender {
		$( $meetup_sender_ty:ty = [ $( $meetup_sender_cur:literal )* ] ),* $(,)?
	}
) {
	$(
		$(
			impl AsyncMutexLocker<$async_mutex_ty> for Locker<$async_mutex_cur> {
				const NEXT_STATE: usize = $async_mutex_next;
			}
		)*
	)*

	$(
		$(
			impl AsyncRwLockLocker<$async_rwlock_ty> for Locker<$async_rwlock_cur> {
				const NEXT_STATE: usize = $async_rwlock_next;
			}
		)*
	)*

	$(
		$(
			impl MeetupSenderLocker<$meetup_sender_ty> for Locker<$meetup_sender_cur> {

			}
		)*
	)*
}
