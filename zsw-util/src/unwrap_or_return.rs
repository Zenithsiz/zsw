//! Unwrap or return

// Imports
use std::ops::{ControlFlow, FromResidual, Try};

/// `Try` implementor for [`UnwrapOrReturnExt::unwrap_or_return`]
#[derive(Clone, Copy, Debug)]
pub enum UnwrapOrReturn<T, E> {
	/// Return value
	Value(T),

	/// Should return
	Return(E),
}

impl<T, E> Try for UnwrapOrReturn<T, E> {
	type Output = T;
	type Residual = UnwrapOrReturn<!, E>;

	fn from_output(output: Self::Output) -> Self {
		Self::Value(output)
	}

	fn branch(self) -> ControlFlow<Self::Residual, Self::Output> {
		match self {
			Self::Value(value) => ControlFlow::Continue(value),
			Self::Return(ret) => ControlFlow::Break(UnwrapOrReturn::Return(ret)),
		}
	}
}

impl<T, E> FromResidual<UnwrapOrReturn<!, E>> for UnwrapOrReturn<T, E> {
	fn from_residual(residual: UnwrapOrReturn<!, E>) -> Self {
		match residual {
			UnwrapOrReturn::Value(never) => never,
			UnwrapOrReturn::Return(ret) => Self::Return(ret),
		}
	}
}

// TODO: Support `impl<E> FromResidual<UnwrapOrReturn<!, E>> for E`?
impl FromResidual<UnwrapOrReturn<!, ()>> for () {
	fn from_residual(residual: UnwrapOrReturn<!, ()>) -> Self {
		match residual {
			UnwrapOrReturn::Value(never) => never,
			UnwrapOrReturn::Return(_) => (),
		}
	}
}

#[extend::ext(name = UnwrapOrReturnExt)]
pub impl<T, E> Result<T, E> {
	/// Unwraps this result, or returns a value that can be `?`d to return `()`.
	// TODO: Allow any return `R: Default` or similar?
	fn unwrap_or_return(self) -> UnwrapOrReturn<T, E> {
		match self {
			Ok(value) => UnwrapOrReturn::Value(value),
			Err(err) => UnwrapOrReturn::Return(err),
		}
	}
}


#[cfg(test)]
mod test {
	use std::assert_matches::assert_matches;

	use super::*;

	#[test]
	fn ok() {
		let res = Ok::<_, &str>(());
		assert_matches!(res.unwrap_or_return(), UnwrapOrReturn::Value(()));
	}

	#[test]
	fn err() {
		let res = Err::<(), &str>("error");
		assert_matches!(res.unwrap_or_return(), UnwrapOrReturn::Return("error"));
	}

	#[test]
	fn ret() {
		let res = Err::<(), ()>(());

		res.unwrap_or_return()?;
		unreachable!("Should not be reached")
	}
}
