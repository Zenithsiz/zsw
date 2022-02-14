//! Arguments

// Imports
use std::path::PathBuf;

/// Arguments
#[derive(Debug)]
#[derive(clap::Parser)]
pub struct Args {
	/// Profile to load
	#[clap(long = "profile", short = 'p')]
	pub profile: Option<PathBuf>,
}
