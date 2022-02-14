//! External crate wrappers with side effects
//!
//! `EXTern Side Effect`

// Imports
use super::{MightBlock, WithSideEffect};

/// Side effect wrapper for [`crossbeam::channel::Receiver`]
pub trait CrossBeamChannelReceiverSE<T> {
	fn recv_se(&self) -> WithSideEffect<Result<T, crossbeam::channel::RecvError>, MightBlock>;
}

impl<T> CrossBeamChannelReceiverSE<T> for crossbeam::channel::Receiver<T> {
	fn recv_se(&self) -> WithSideEffect<Result<T, crossbeam::channel::RecvError>, MightBlock> {
		#[allow(clippy::disallowed_methods)] // We're wrapping it
		WithSideEffect::new(self.recv())
	}
}

/// Side effect wrapper for [`crossbeam::channel::Sender`]
pub trait CrossBeamChannelSenderSE<T> {
	fn send_se(&self, msg: T) -> WithSideEffect<Result<(), crossbeam::channel::SendError<T>>, MightBlock>;
}

impl<T> CrossBeamChannelSenderSE<T> for crossbeam::channel::Sender<T> {
	fn send_se(&self, msg: T) -> WithSideEffect<Result<(), crossbeam::channel::SendError<T>>, MightBlock> {
		#[allow(clippy::disallowed_methods)] // We're wrapping it
		WithSideEffect::new(self.send(msg))
	}
}

/// Side effect wrapper for [`crossbeam::channel::Select`]
pub trait CrossBeamChannelSelectSE<'a> {
	fn select_se(&mut self) -> WithSideEffect<crossbeam::channel::SelectedOperation<'a>, MightBlock>;
}

impl<'a> CrossBeamChannelSelectSE<'a> for crossbeam::channel::Select<'a> {
	fn select_se(&mut self) -> WithSideEffect<crossbeam::channel::SelectedOperation<'a>, MightBlock> {
		#[allow(clippy::disallowed_methods)] // We're wrapping it
		WithSideEffect::new(self.select())
	}
}

/// Side effect wrapper for [`parking_lot::Mutex`]
pub trait ParkingLotMutexSe<T> {
	fn lock_se(&self) -> WithSideEffect<parking_lot::MutexGuard<'_, T>, MightBlock>;
}

impl<T> ParkingLotMutexSe<T> for parking_lot::Mutex<T> {
	fn lock_se(&self) -> WithSideEffect<parking_lot::MutexGuard<'_, T>, MightBlock> {
		#[allow(clippy::disallowed_methods)] // We're wrapping it
		WithSideEffect::new(self.lock())
	}
}
