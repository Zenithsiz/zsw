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
	const_trait_impl,
	nonpoison_mutex,
	sync_nonpoison,
	async_fn_traits,
	trait_alias,
	unboxed_closures,
	tuple_trait,
	try_blocks
)]

// Modules
pub mod duration_display;
pub mod loadable;
mod rect;
pub mod resource_manager;
mod tuple_collect_res;
pub mod unwrap_or_return;
pub mod walk_dir;

// Exports
pub use {
	duration_display::DurationDisplay,
	loadable::Loadable,
	rect::Rect,
	resource_manager::ResourceManager,
	tuple_collect_res::{TupleCollectRes1, TupleCollectRes2, TupleCollectRes3, TupleCollectRes4, TupleCollectRes5},
	unwrap_or_return::{UnwrapOrReturn, UnwrapOrReturnExt},
	walk_dir::WalkDir,
};

// Imports
use {
	app_error::Context,
	image::DynamicImage,
	serde::de::DeserializeOwned,
	std::{fs, future::Future, path::Path},
};

/// App error export with our data
pub type AppError = app_error::AppError<()>;

/// Parses json from a file
pub fn parse_json_from_file<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T, AppError> {
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
	#[track_caller]
	fn block_on(self) -> F::Output {
		let handle = tokio::runtime::Handle::current();
		handle.block_on(self)
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
pub const fn array_max<const N: usize>(values: &[usize; N]) -> Option<usize> {
	let mut max = None;
	let mut cur_idx = 0;
	while cur_idx < values.len() {
		let value = values[cur_idx];

		max = Some(match max {
			Some(max) => self::usize_max(max, value),
			None => value,
		});

		cur_idx += 1;
	}

	max
}

/// Returns the maximum between two `usize` values
const fn usize_max(lhs: usize, rhs: usize) -> usize {
	if lhs > rhs { lhs } else { rhs }
}
