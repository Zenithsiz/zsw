//! Side effect

// Module
pub mod extse;

// Imports
use std::marker::PhantomData;

/// A value with side effect
#[derive(Debug)]
#[must_use = "This value indicates a function has side effects. You must call `.allow::<SideEffects>()` to allow the \
              side effects and retrieve the inner value"]
pub struct WithSideEffect<Value, Effects> {
	/// Value
	value: Value,

	/// Effects
	effects: PhantomData<Effects>,
}

impl<Value, Effects: SideEffect> WithSideEffect<Value, Effects> {
	/// Creates a new side effect from `value`
	pub fn new(value: Value) -> Self {
		Self {
			value,
			effects: PhantomData,
		}
	}

	/// Maps this side effect
	pub fn map<MappedValue>(self, f: impl FnOnce(Value) -> MappedValue) -> WithSideEffect<MappedValue, Effects> {
		WithSideEffect {
			value:   f(self.value),
			effects: self.effects,
		}
	}

	/// Converts a `&WithSideEffect<T, S>` to `WithSideEffect<&T, S>`
	pub fn as_ref(&self) -> WithSideEffect<&Value, Effects> {
		WithSideEffect {
			value:   &self.value,
			effects: self.effects,
		}
	}

	/// Allows all effects and returns the inner value
	///
	/// This must be used with turbofish to ensure that you write the side
	/// effects so they may be grep-able.
	pub fn allow<AllowedEffects>(self) -> Value
	where
		AllowedEffects: Eq<AllowedEffects>,
	{
		self.value
	}
}

/// A side effect
// Note: Not technically required, just so we don't put anything in the second generic
//       of `WithSideEffect` that we don't intend.
pub trait SideEffect {}

// Note: Tuples of effects are effects
impl<E1: SideEffect> SideEffect for (E1,) {}
impl<E1: SideEffect, E2: SideEffect> SideEffect for (E1, E2) {}
impl<E1: SideEffect, E2: SideEffect, E3: SideEffect, E4: SideEffect> SideEffect for (E1, E2, E3, E4) {}

/// Side effect to indicate that a function might deadlock.
#[derive(Clone, Copy, Debug)]
pub struct MightDeadlock;

impl SideEffect for MightDeadlock {}

/// Trait to check if two types are equal
// TODO: Possibly make this sealed?
pub trait Eq<T> {}
impl<T> Eq<T> for T {}
