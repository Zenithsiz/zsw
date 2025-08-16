//! Utility

// Features
#![feature(
	decl_macro,
	coroutine_trait,
	coroutines,
	never_type,
	type_alias_impl_trait,
	if_let_guard,
	extend_one,
	must_not_suspend,
	impl_trait_in_assoc_type,
	try_trait_v2,
	assert_matches,
	yeet_expr,
	const_trait_impl
)]

// Modules
pub mod meetup;
mod rect;
mod tuple_collect_res;
pub mod unwrap_or_return;
pub mod walk_dir;

// Exports
pub use {
	rect::Rect,
	tuple_collect_res::{TupleCollectRes1, TupleCollectRes2, TupleCollectRes3, TupleCollectRes4, TupleCollectRes5},
	unwrap_or_return::{UnwrapOrReturn, UnwrapOrReturnExt},
	walk_dir::WalkDir,
};

// Imports
use {
	image::DynamicImage,
	std::{
		ffi::OsStr,
		fs,
		future::Future,
		path::{Path, PathBuf},
	},
	zutil_app_error::{AppError, Context},
};

/// Parses json from a file
pub fn parse_json_from_file<T: serde::de::DeserializeOwned>(path: impl AsRef<Path>) -> Result<T, AppError> {
	// Open the file
	let file = fs::File::open(path).context("Unable to open file")?;

	// Then parse it
	serde_json::from_reader(file).context("Unable to parse file")
}

/// Serializes json to a file
pub fn serialize_json_to_file<T: serde::Serialize>(path: impl AsRef<Path>, value: &T) -> Result<(), AppError> {
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

/// Appends a string to this path
#[extend::ext(name = PathAppendExt)]
pub impl PathBuf {
	/// Appends a string to this path
	fn with_appended<S: AsRef<OsStr>>(mut self, s: S) -> Self {
		self.as_mut_os_string().push(s);
		self
	}
}

/// Ensures `cond` is true in a `where` clause
pub macro where_assert($cond:expr) {
	// Note: If `true`, this expands to `[(); 0]`, which is valid
	//       If `false`, it expands to `[(); -1]`, which is invalid
	[(); ($cond as usize) - 1]
}

/// Blocks on a future inside a tokio task
#[extend::ext(name = TokioTaskBlockOn)]
pub impl<F: Future> F {
	/// Bocks on this future within a tokio task
	fn block_on(self) -> F::Output {
		tokio::task::block_in_place(move || {
			let handle = tokio::runtime::Handle::current();
			handle.block_on(self)
		})
	}
}

/// Logs an error and panics with the error message
pub macro log_error_panic( $($rest:tt)* ) {{
	::tracing::warn!( $($rest)* );

	// TODO: Better way of getting the message as the last argument?
	let (.., msg) = ( $( stringify!($rest) ),* );
	let msg = &msg[1..];
	let msg = &msg[..msg.len() - 1];

	::std::panic!("{msg}");
}}

/// Returns the maximum value in an array as a `const fn`
#[must_use]
#[expect(clippy::missing_panics_doc, reason = "It's an internal panic")]
pub const fn array_max<const N: usize>(values: &[usize; N]) -> Option<usize> {
	let mut max = None;
	let mut cur_idx = 0;
	while cur_idx < values.len() {
		#[expect(
			clippy::unwrap_used,
			reason = "We know it's `Some` in that branch and we can't use any pattern matching or `Option` methods"
		)]
		if max.is_none() || values[cur_idx] > max.unwrap() {
			max = Some(values[cur_idx]);
		}

		cur_idx += 1;
	}

	max
}
