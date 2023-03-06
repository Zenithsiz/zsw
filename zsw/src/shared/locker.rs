//! Locker

// Lints
#![expect(
	clippy::disallowed_methods,
	reason = "DEADLOCK: We ensure thread safety via the locker abstraction"
)]

// Imports
use {
	crate::{
		panel::{PanelGroup, PanelsRendererShader},
		playlist::Playlists,
	},
	tokio::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard},
	zsw_util::meetup,
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

/// Async mutex `tokio::sync::Mutex<R>` resource
#[sealed::sealed]
pub trait AsyncMutexResource {
	/// Inner type
	type Inner;

	/// Returns the inner mutex
	fn as_inner(&self) -> &Mutex<Self::Inner>;
}

/// Async mutex `tokio::sync::Mutex<R>` resource extension trait
#[extend::ext(name = AsyncMutexResourceExt)]
#[sealed::sealed]
pub impl<R: AsyncMutexResource> R {
	/// Locks this mutex
	#[track_caller]
	async fn lock<'locker, const STATE: usize>(
		&'locker self,
		_locker: &'locker mut Locker<STATE>,
	) -> (
		MutexGuard<'locker, R::Inner>,
		Locker<{ <Locker<STATE> as AsyncMutexLocker<R>>::NEXT_STATE }>,
	)
	where
		Locker<STATE>: AsyncMutexLocker<R>,
		R::Inner: 'locker,
		[(); <Locker<STATE> as AsyncMutexLocker<R>>::NEXT_STATE]:,
	{
		let guard = self.as_inner().lock().await;
		(guard, Locker(()))
	}

	/// Locks this mutex blockingly
	#[track_caller]
	fn blocking_lock<'locker, const STATE: usize>(
		&'locker self,
		_locker: &'locker mut Locker<STATE>,
	) -> (
		MutexGuard<'locker, R::Inner>,
		Locker<{ <Locker<STATE> as AsyncMutexLocker<R>>::NEXT_STATE }>,
	)
	where
		Locker<STATE>: AsyncMutexLocker<R>,
		R::Inner: 'locker,
		[(); <Locker<STATE> as AsyncMutexLocker<R>>::NEXT_STATE]:,
	{
		let guard = self.as_inner().blocking_lock();
		(guard, Locker(()))
	}
}

/// Locker for `tokio::sync::Mutex<R>`
#[sealed::sealed]
pub trait AsyncMutexLocker<R> {
	const NEXT_STATE: usize;
}

/// Async rwlock `tokio::sync::RwLock<R>` resource
#[sealed::sealed]
pub trait AsyncRwLockResource {
	/// Inner type
	type Inner;

	/// Returns the inner rwlock
	fn as_inner(&self) -> &RwLock<Self::Inner>;
}

/// Async rwlock `tokio::sync::RwLock<R>` resource extension trait
#[extend::ext(name = AsyncRwLockResourceExt)]
#[sealed::sealed]
pub impl<R: AsyncRwLockResource> R {
	/// Locks this rwlock for reads
	#[track_caller]
	async fn read<'locker, const STATE: usize>(
		&'locker self,
		_locker: &'locker mut Locker<STATE>,
	) -> (
		RwLockReadGuard<'locker, R::Inner>,
		Locker<{ <Locker<STATE> as AsyncRwLockLocker<R>>::NEXT_STATE }>,
	)
	where
		Locker<STATE>: AsyncRwLockLocker<R>,
		R::Inner: 'locker,
		[(); <Locker<STATE> as AsyncRwLockLocker<R>>::NEXT_STATE]:,
	{
		let guard = self.as_inner().read().await;
		(guard, Locker(()))
	}

	/// Locks this rwlock for reads blockingly
	#[track_caller]
	fn blocking_read<'locker, const STATE: usize>(
		&'locker self,
		_locker: &'locker mut Locker<STATE>,
	) -> (
		RwLockReadGuard<'locker, R::Inner>,
		Locker<{ <Locker<STATE> as AsyncRwLockLocker<R>>::NEXT_STATE }>,
	)
	where
		Locker<STATE>: AsyncRwLockLocker<R>,
		R::Inner: 'locker,
		[(); <Locker<STATE> as AsyncRwLockLocker<R>>::NEXT_STATE]:,
	{
		let guard = self.as_inner().blocking_read();
		(guard, Locker(()))
	}

	/// Locks this rwlock for writes
	#[track_caller]
	async fn write<'locker, const STATE: usize>(
		&'locker self,
		_locker: &'locker mut Locker<STATE>,
	) -> (
		RwLockWriteGuard<'locker, R::Inner>,
		Locker<{ <Locker<STATE> as AsyncRwLockLocker<R>>::NEXT_STATE }>,
	)
	where
		Locker<STATE>: AsyncRwLockLocker<R>,
		R::Inner: 'locker,
		[(); <Locker<STATE> as AsyncRwLockLocker<R>>::NEXT_STATE]:,
	{
		let guard = self.as_inner().write().await;
		(guard, Locker(()))
	}

	/// Locks this rwlock for writes blockingly
	#[track_caller]
	fn blocking_write<'locker, const STATE: usize>(
		&'locker self,
		_locker: &'locker mut Locker<STATE>,
	) -> (
		RwLockWriteGuard<'locker, R::Inner>,
		Locker<{ <Locker<STATE> as AsyncRwLockLocker<R>>::NEXT_STATE }>,
	)
	where
		Locker<STATE>: AsyncRwLockLocker<R>,
		R::Inner: 'locker,
		[(); <Locker<STATE> as AsyncRwLockLocker<R>>::NEXT_STATE]:,
	{
		let guard = self.as_inner().blocking_write();
		(guard, Locker(()))
	}
}

/// Locker for `tokio::sync::RwLock<R>`
#[sealed::sealed]
pub trait AsyncRwLockLocker<R> {
	const NEXT_STATE: usize;
}

/// Meetup sender `zsw_util::meetup::Sender<R>` resource
#[sealed::sealed]
pub trait MeetupSenderResource {
	/// Inner type
	type Inner;

	/// Returns the inner meetup sender
	fn as_inner(&self) -> &meetup::Sender<Self::Inner>;
}

/// Meetup sender `zsw_util::meetup::Sender<R>` resource extension trait
#[extend::ext(name = MeetupSenderResourceExt)]
#[sealed::sealed]
pub impl<R: MeetupSenderResource> R {
	/// Sends the resource `R` to this meetup channel
	#[track_caller]
	async fn send<'locker, const STATE: usize>(&'locker self, _locker: &'locker mut Locker<STATE>, resource: R::Inner)
	where
		Locker<STATE>: MeetupSenderLocker<R>,
	{
		self.as_inner().send(resource).await;
	}
}

/// Locker for `zsw_util::meetup::Sender<R>`
// Note: No `NEXT_STATE`, as we don't keep anything locked.
#[sealed::sealed]
pub trait MeetupSenderLocker<R> {}


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
		#[derive(Debug)]
		pub struct $async_mutex_name(Mutex<$async_mutex_ty>);

		impl $async_mutex_name {
			/// Creates the mutex
			pub fn $new(inner: $async_mutex_ty) -> Self {
				Self(Mutex::new(inner))
			}
		}

		#[sealed::sealed]
		impl AsyncMutexResource for $async_mutex_name {
			type Inner = $async_mutex_ty;

			fn as_inner(&self) -> &Mutex<Self::Inner> {
				&self.0
			}
		}


		$(
			#[sealed::sealed]
			impl AsyncMutexLocker<$async_mutex_name> for Locker<$async_mutex_cur> {
				const NEXT_STATE: usize = $async_mutex_next;
			}
		)*
	)*

	$(
		#[derive(Debug)]
		pub struct $async_rwlock_name(RwLock<$async_rwlock_ty>);

		impl $async_rwlock_name {
			/// Creates the rwlock
			pub fn $new(inner: $async_rwlock_ty) -> Self {
				Self(RwLock::new(inner))
			}
		}

		#[sealed::sealed]
		impl AsyncRwLockResource for $async_rwlock_name {
			type Inner = $async_rwlock_ty;

			fn as_inner(&self) -> &RwLock<Self::Inner> {
				&self.0
			}
		}

		$(
			#[sealed::sealed]
			impl AsyncRwLockLocker<$async_rwlock_name> for Locker<$async_rwlock_cur> {
				const NEXT_STATE: usize = $async_rwlock_next;
			}
		)*
	)*

	$(
		#[derive(Debug)]
		pub struct $meetup_sender_name(meetup::Sender<$meetup_sender_ty>);

		impl $meetup_sender_name {
			/// Creates the meetup sender
			// TODO: Not receive a built sender and instead create a `(Sender, Receiver)` pair?
			pub fn $new(inner: meetup::Sender<$meetup_sender_ty>) -> Self {
				Self(inner)
			}
		}

		#[sealed::sealed]
		impl MeetupSenderResource for $meetup_sender_name {
			type Inner = $meetup_sender_ty;

			fn as_inner(&self) -> &meetup::Sender<Self::Inner> {
				&self.0
			}
		}

		$(
			#[sealed::sealed]
			impl MeetupSenderLocker<$meetup_sender_name> for Locker<$meetup_sender_cur> { }
		)*
	)*
}
