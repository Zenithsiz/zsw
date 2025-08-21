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

	/// Log file
	///
	/// Specifies a file to perform verbose logging to.
	/// You can use `RUST_FILE_LOG` to set filtering options
	#[clap(long = "log-file")]
	pub log_file: Option<PathBuf>,
}
