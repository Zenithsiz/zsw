//! Arguments

// Imports
use std::path::PathBuf;

/// Arguments
#[derive(Debug)]
#[derive(clap::Parser)]
pub struct Args {
	/// Config file
	///
	/// Overrides the default config file
	#[clap(long = "config")]
	pub config: Option<PathBuf>,
}
