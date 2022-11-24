//! Utility

// Features
#![feature(
	decl_macro,
	generator_trait,
	generators,
	never_type,
	type_alias_impl_trait,
	async_fn_in_trait
)]
#![allow(incomplete_features)]

use std::path::PathBuf;

// Modules
mod condvar_future;
mod display_wrapper;
mod fetch_update;
mod lock;
mod rect;
mod resource;
mod service;
mod thread;

// Exports
pub use {
	condvar_future::CondvarFuture,
	display_wrapper::DisplayWrapper,
	fetch_update::FetchUpdate,
	lock::Lock,
	rect::Rect,
	resource::{Resources, ResourcesBundle, ResourcesTuple2},
	service::{Services, ServicesBundle},
	thread::ThreadSpawner,
};

// Imports
use {
	anyhow::Context,
	image::DynamicImage,
	std::{
		fs,
		future::Future,
		path::Path,
		time::{Duration, Instant},
	},
};

/// Measures how long it took to execute a function
pub fn measure<T>(f: impl FnOnce() -> T) -> (T, Duration) {
	let start_time = Instant::now();
	let value = f();
	let duration = start_time.elapsed();
	(value, duration)
}

/// Measures how long it took to execute an async function
pub async fn measure_async<F: Future + Send>(f: F) -> (F::Output, Duration) {
	let start_time = Instant::now();
	let value = f.await;
	let duration = start_time.elapsed();
	(value, duration)
}

/// Measures how long it took to execute a fallible async function
pub async fn try_measure_async<T, E, F: Future<Output = Result<T, E>> + Send>(f: F) -> Result<(T, Duration), E> {
	let start_time = Instant::now();
	let value = f.await?;
	let duration = start_time.elapsed();
	Ok((value, duration))
}

/// Measures how long it took to execute a statement
pub macro measure($value:expr) {{
	let start_time = ::std::time::Instant::now();
	match $value {
		value => {
			let duration = ::std::time::Instant::elapsed(&start_time);
			(value, duration)
		},
	}
}}

/// Measures how long it took to execute a fallible statement,
/// returning a `Result<(T, Duration), Err>`
pub macro try_measure($value:expr) {{
	let start_time = ::std::time::Instant::now();
	match $value {
		::std::result::Result::Ok(value) => {
			let duration = ::std::time::Instant::elapsed(&start_time);
			::std::result::Result::Ok((value, duration))
		},
		::std::result::Result::Err(err) => ::std::result::Result::Err(err),
	}
}}

/// Helper trait to measure a future
pub trait MeasureFuture: Future {
	/// Future type
	type Fut: Future<Output = (Self::Output, Duration)>;

	/// Measures this future's execution
	fn measure_fut(self) -> Self::Fut;
}

impl<F: Future + Send> MeasureFuture for F {
	type Fut = impl Future<Output = (F::Output, Duration)>;

	fn measure_fut(self) -> Self::Fut {
		self::measure_async(self)
	}
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

/// Serializes json to a file
pub fn serialize_json_to_file<T: serde::ser::Serialize>(
	path: impl AsRef<Path>,
	value: &T,
) -> Result<(), anyhow::Error> {
	// Open the file
	let file = fs::File::create(path).context("Unable to create file")?;

	// Then serialize it
	serde_json::to_writer_pretty(file, value).context("Unable to serialize to file")
}

/// Returns the image format string of an image (for logging)
#[must_use]
pub fn image_format(image: &DynamicImage) -> &'static str {
	match image {
		DynamicImage::ImageLuma8(_) => "Luma8",
		DynamicImage::ImageLumaA8(_) => "LumaA8",
		DynamicImage::ImageRgb8(_) => "Rgb8",
		DynamicImage::ImageRgba8(_) => "Rgba8",
		DynamicImage::ImageLuma16(_) => "Luma16",
		DynamicImage::ImageLumaA16(_) => "LumaA16",
		DynamicImage::ImageRgb16(_) => "Rgb16",
		DynamicImage::ImageRgba16(_) => "Rgba16",
		_ => "<unknown>",
	}
}

/// Visits all files in `path` recursively
pub fn visit_dir<P: AsRef<Path>>(path: &P, visitor: &mut impl FnMut(PathBuf)) {
	let path = path.as_ref();

	// Try to read the directory
	let dir = match std::fs::read_dir(path) {
		Ok(dir) => dir,
		Err(err) => {
			tracing::warn!(?path, ?err, "Unable to read directory");
			return;
		},
	};

	// Then go through each entry
	for entry in dir {
		// Read the entry and file type
		let entry = match entry {
			Ok(entry) => entry,
			Err(err) => {
				tracing::warn!(?path, ?err, "Unable to read file entry");
				continue;
			},
		};
		let entry_path = entry.path();
		let file_type = match entry.file_type() {
			Ok(file_type) => file_type,
			Err(err) => {
				tracing::warn!(?entry_path, ?err, "Unable to read file type",);
				continue;
			},
		};

		match file_type.is_dir() {
			// Recurse on directories
			true => self::visit_dir(&entry_path, &mut *visitor),

			// Yield files
			false => visitor(entry_path),
		}
	}
}


// TODO: Move these elsewhere?
#[must_use]
pub fn default_panel_parallax_ratio() -> f32 {
	1.0
}

#[must_use]
pub fn default_panel_parallax_exp() -> f32 {
	2.0
}

#[must_use]
pub fn default_panel_parallax_reverse() -> bool {
	false
}
