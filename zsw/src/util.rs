//! Utility

// Modules
mod display_wrapper;
mod scan_dir;
mod side_effect;
mod thread;

// Exports
pub use {
	display_wrapper::DisplayWrapper,
	scan_dir::visit_files_dir,
	side_effect::{extse, MightBlock, SideEffect, WithSideEffect},
	thread::ThreadSpawner,
};

// Imports
use {
	anyhow::Context,
	image::DynamicImage,
	std::{
		fs,
		hash::{Hash, Hasher},
		path::Path,
		time::{Duration, Instant},
	},
};

/// Measures how long it took to execute a function
pub fn measure<T>(f: impl FnOnce() -> T) -> (T, Duration) {
	let start_time = Instant::now();
	let value = f();
	let duration = Instant::now().saturating_duration_since(start_time);
	(value, duration)
}

pub macro measure_dbg {
	() => {
		::std::eprintln!("[{}:{}]", ::std::file!(), ::std::line!())
	},
	($value:expr $(,)?) => {
		match $crate::util::measure(|| $value) {
			(value, duration) => {
				::std::eprintln!("[{}:{}] {} took {:?}",
					::std::file!(), ::std::line!(), ::std::stringify!($value), duration);
				value
			}
		}
	},
	($($val:expr),+ $(,)?) => {
		($(::std::dbg!($val)),+,)
	}
}

/// Parses json from a file
pub fn parse_json_from_file<T: serde::de::DeserializeOwned>(path: impl AsRef<Path>) -> Result<T, anyhow::Error> {
	// Open the file
	let file = fs::File::open(path).context("Unable to open file")?;

	// Then parse it
	serde_json::from_reader(file).context("Unable to parse file")
}

/// Hashes a value using `twox_hash`
pub fn _hash_of<T: ?Sized + Hash>(value: &T) -> u64 {
	let mut hasher = twox_hash::XxHash64::with_seed(0);
	value.hash(&mut hasher);
	hasher.finish()
}

/// Returns the image format string of an image (for logging)
pub fn image_format(image: &DynamicImage) -> &'static str {
	match image {
		DynamicImage::ImageLuma8(_) => "Luma8",
		DynamicImage::ImageLumaA8(_) => "LumaA8",
		DynamicImage::ImageRgb8(_) => "Rgb8",
		DynamicImage::ImageRgba8(_) => "Rgba8",
		DynamicImage::ImageBgr8(_) => "Bgr8",
		DynamicImage::ImageBgra8(_) => "Bgra8",
		DynamicImage::ImageLuma16(_) => "Luma16",
		DynamicImage::ImageLumaA16(_) => "LumaA16",
		DynamicImage::ImageRgb16(_) => "Rgb16",
		DynamicImage::ImageRgba16(_) => "Rgba16",
	}
}
