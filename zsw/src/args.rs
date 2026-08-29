//! Arguments

use {crate::profile::ProfileName, std::path::PathBuf};

/// Arguments
#[derive(Debug)]
#[derive(clap::Parser)]
pub struct Args {
	/// Config file
	///
	/// Overrides the default config file
	#[clap(long)]
	pub config: Option<PathBuf>,

	/// Log file
	///
	/// Specifies a file to perform verbose logging to.
	/// You can use `RUST_FILE_LOG` to set filtering options
	#[clap(long)]
	pub log_file: Option<PathBuf>,

	/// Forces usage of the OpenGL backend.
	#[clap(long)]
	pub force_opengl: bool,

	/// Profile to start with
	pub profile: ProfileName,
}
