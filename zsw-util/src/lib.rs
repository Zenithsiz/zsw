//! Utility

// Features
#![feature(
	decl_macro,
	generator_trait,
	generators,
	scoped_threads,
	mixed_integer_ops,
	never_type,
	generic_associated_types,
	type_alias_impl_trait
)]
// Lints
#![warn(
	clippy::pedantic,
	clippy::nursery,
	missing_copy_implementations,
	missing_debug_implementations,
	noop_method_call,
	unused_results
)]
#![deny(
	// We want to annotate unsafe inside unsafe fns
	unsafe_op_in_unsafe_fn,
	// We muse use `expect` instead
	clippy::unwrap_used
)]
#![allow(
	// Style
	clippy::implicit_return,
	clippy::multiple_inherent_impl,
	clippy::pattern_type_mismatch,
	// `match` reads easier than `if / else`
	clippy::match_bool,
	clippy::single_match_else,
	//clippy::single_match,
	clippy::self_named_module_files,
	clippy::items_after_statements,
	clippy::module_name_repetitions,
	// Performance
	clippy::suboptimal_flops, // We prefer readability
	// Some functions might return an error in the future
	clippy::unnecessary_wraps,
	// Due to working with windows and rendering, which use `u32` / `f32` liberally
	// and interchangeably, we can't do much aside from casting and accepting possible
	// losses, although most will be lossless, since we deal with window sizes and the
	// such, which will fit within a `f32` losslessly.
	clippy::cast_precision_loss,
	clippy::cast_possible_truncation,
	// We use proper error types when it matters what errors can be returned, else,
	// such as when using `anyhow`, we just assume the caller won't check *what* error
	// happened and instead just bubbles it up
	clippy::missing_errors_doc,
	// Too many false positives and not too important
	clippy::missing_const_for_fn,
	// This is a binary crate, so we don't expose any API
	rustdoc::private_intra_doc_links,
)]

// Modules
mod display_wrapper;
mod lock;
mod rect;
mod scan_dir;
mod side_effect;
mod thread;

// Exports
pub use {
	display_wrapper::DisplayWrapper,
	lock::Lock,
	rect::Rect,
	scan_dir::dir_files_iter,
	side_effect::{extse, MightBlock, SideEffect, WithSideEffect},
	thread::{FutureRunner, ThreadSpawner},
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
		DynamicImage::ImageRgb32F(_) => "Rgb32f",
		DynamicImage::ImageRgba32F(_) => "Rgba32f",
		_ => "<other>",
	}
}