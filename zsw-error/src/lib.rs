//! App error

// Features
#![feature(must_not_suspend, strict_provenance, never_type)]

// Imports
use std::{io, sync::Arc};

/// App error
#[derive(Debug, thiserror::Error)]
pub enum AppError {
	/// Shared
	#[error(transparent)]
	Shared(Arc<Self>),

	/// Other
	// TODO: Remove `from` and make it so we need to use `map_err(AppError::Other)`
	#[error(transparent)]
	Other(#[from] anyhow::Error),

	/// Io error
	// TODO: Separate this into several sub-errors like `ReadFile`, `ReadMetadata`, etc...
	#[error("Io error")]
	Io(#[source] io::Error),
}

impl From<!> for AppError {
	fn from(never: !) -> Self {
		never
	}
}
