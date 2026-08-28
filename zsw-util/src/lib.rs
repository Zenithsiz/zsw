//! Utility

#![feature(
	decl_macro,
	must_not_suspend,
	const_trait_impl,
	unboxed_closures,
	proc_macro_hygiene,
	stmt_expr_attributes,
	core_intrinsics,
	current_thread_id,
	oneshot_channel,
	extend_one
)]
#![expect(internal_features, reason = "There's no other way to check if a type is inhabited")]

pub mod loadable;
mod rect;
mod tuple_collect_res;
mod walk_dir;

pub use {
	self::{
		loadable::Loadable,
		rect::Rect,
		tuple_collect_res::{TupleCollectRes1, TupleCollectRes2, TupleCollectRes3, TupleCollectRes4, TupleCollectRes5},
		walk_dir::WalkDir,
	},
	zsw_util_macros::*,
};

use {
	app_error::Context,
	core::{ptr, str::FromStr},
	image::DynamicImage,
	serde::de::DeserializeOwned,
	std::{ffi::OsStr, fs, intrinsics, path::Path, thread},
	zutil_cloned::cloned,
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

/// Spawns a task
#[track_caller]
pub fn spawn_task<F>(name: impl Into<String>, f: F)
where
	F: FnOnce() -> Result<(), AppError> + Send + 'static,
{
	let name = name.into();

	#[cloned(name)]
	let f = move || {
		let id = thread::current_id();
		tracing::debug!("Spawning task {name:?} ({id:?})");
		match f() {
			Ok(()) => tracing::debug!("Task {name:?} ({id:?}) finished"),
			Err(err) => tracing::warn!("Task {name:?} ({id:?}) returned error: {err:?}"),
		}
	};

	if let Err(err) = thread::Builder::new().name(name.clone()).spawn(f) {
		let err = AppError::new(&err);
		tracing::warn!("Unable to spawn task {name:?}: {err:?}");
	}
}

/// Iterator chain
pub macro iter_chain {
	($only:expr $(,)?) => {
		$only
	},

	($first:expr, $($rest:expr),* $(,)?) => {
		std::iter::chain($first, $crate::iter_chain!($($rest,)*))
	},
}

/// Creates a mutable reference to a ZST
#[must_use]
pub const fn zst_ref_mut<'a, T>() -> &'a mut T {
	const { assert!(size_of::<T>() == 0, "Cannot call this function with non-zero `T`") };
	const { intrinsics::assert_inhabited::<T>() };

	// SAFETY: `T` is a ZST and is inhabited, so this is valid
	unsafe { &mut *ptr::dangling_mut() }
}

/// Reads all toml files in a directory as values.
///
/// The key will be their name, excluding the `.toml` extension.
pub fn read_dir_all_toml<K, V, R>(dir: &Path) -> Result<R, AppError>
where
	K: FromStr + Send + 'static,
	V: DeserializeOwned + Send + Sync + 'static,
	R: Default + Extend<(K, V)>,
	// TODO: This bound is ugly, can we make it better?
	Result<K, K::Err>: Context<(), Output = Result<K, AppError>>,
{
	fs::create_dir_all(dir).context("Unable to create root directory")?;
	let dir = fs::read_dir(dir).context("Unable to read directory")?;

	let mut values = R::default();
	for entry in dir {
		// Ignore directories and non `.toml` files
		let entry = entry.context("Unable to get entry")?;
		let entry_path = entry.path();
		if entry.file_type().context("Unable to get entry metadata")?.is_dir() ||
			entry_path.extension().and_then(OsStr::to_str) != Some("toml")
		{
			continue;
		}

		// Then get the name from the file
		let name = entry_path.file_stem().context("Entry path had no file stem")?;
		let name = name
			.to_str()
			.with_context(|| format!("Entry name was non-utf8: {name:?}"))?;
		let name = name
			.parse::<K>()
			.with_context(|| format!("Entry name was invalid {name:?}"))?;

		// Try to read the file
		let toml = fs::read_to_string(&entry_path).with_context(|| format!("Unable to read file {entry_path:?}"))?;

		// And parse it
		let value = toml::from_str(&toml).with_context(|| format!("Unable to parse file {entry_path:?}"))?;

		values.extend_one((name, value));
	}

	Ok(values)
}
