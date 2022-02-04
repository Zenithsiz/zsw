//! External crate wrappers with side effects
//!
//! `EXTern Side Effect`

// Imports
use super::{MightDeadlock, WithSideEffect};

/// Side effect wrapper for [`crossbeam::channel::Receiver`]
pub trait CrossBeamChannelReceiverSE<T> {
	fn recv_se(&self) -> WithSideEffect<Result<T, crossbeam::channel::RecvError>, MightDeadlock>;
}

impl<T> CrossBeamChannelReceiverSE<T> for crossbeam::channel::Receiver<T> {
	fn recv_se(&self) -> WithSideEffect<Result<T, crossbeam::channel::RecvError>, MightDeadlock> {
		#[allow(clippy::disallowed_methods)] // We're wrapping it
		WithSideEffect::new(self.recv())
	}
}

/// Side effect wrapper for [`crossbeam::channel::Sender`]
pub trait CrossBeamChannelSenderSE<T> {
	fn send_se(&self, msg: T) -> WithSideEffect<Result<(), crossbeam::channel::SendError<T>>, MightDeadlock>;
}

impl<T> CrossBeamChannelSenderSE<T> for crossbeam::channel::Sender<T> {
	fn send_se(&self, msg: T) -> WithSideEffect<Result<(), crossbeam::channel::SendError<T>>, MightDeadlock> {
		#[allow(clippy::disallowed_methods)] // We're wrapping it
		WithSideEffect::new(self.send(msg))
	}
}
