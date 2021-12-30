//! Image loader

// Modules
mod downscale_cache;

// Imports
use self::downscale_cache::DownscaleCache;
use super::ImageLoaderArgs;
use crate::img::ImageRequest;
use anyhow::Context;
use cgmath::Vector2;
use image::{imageops::FilterType, io::Reader as ImageReader, DynamicImage};
use num_rational::Ratio;
use std::{
	cmp::Ordering,
	fs,
	io::{BufReader, Seek},
	path::Path,
};

/// Loads an image from a path
pub fn load_image(path: &Path, request: ImageRequest, args: ImageLoaderArgs) -> Result<DynamicImage, anyhow::Error> {
	// Canonicalize the path before loading
	let path = path.canonicalize().context("Unable to canonicalize path")?;

	// Open the image
	let image_file = fs::File::open(&path).context("Unable to open image file")?;
	let mut image_file = BufReader::new(image_file);

	// Then guess it's format
	let format = self::image_format(&mut image_file)?;

	// Get it's size and others
	let image_size = self::image_size(&mut image_file, format)?;

	// Check if we're doing any downscaling
	match self::load_image_downscaled(&path, request, &mut image_file, format, image_size, args) {
		Ok(Some(image)) => return Ok(image),
		Ok(None) => (),
		Err(err) => log::warn!("Unable to load downscaled image {path:?}: {err:?}"),
	}

	// Else, just read the image
	ImageReader::with_format(&mut image_file, format)
		.decode()
		.context("Unable to decode image")
}

/// Returns the image format of `image_file`
// TODO: Something less janky than this
fn image_format(image_file: &mut BufReader<fs::File>) -> Result<image::ImageFormat, anyhow::Error> {
	ImageReader::new(image_file)
		.with_guessed_format()
		.context("Unable to guess image format")?
		.format()
		.context("Image format not supported")
}

/// Returns the image size of `image_file` given it's format
fn image_size(image_file: &mut BufReader<fs::File>, format: image::ImageFormat) -> Result<Vector2<u32>, anyhow::Error> {
	let (width, height) = ImageReader::with_format(&mut *image_file, format)
		.into_dimensions()
		.context("Unable to get image dimensions")?;
	image_file.rewind().context("Unable to rewind file")?;
	Ok(Vector2::new(width, height))
}

/// Loads an image downscaled according to `args`
fn load_image_downscaled(
	path: &Path, request: ImageRequest, image_file: &mut BufReader<fs::File>, format: image::ImageFormat,
	image_size: Vector2<u32>, args: ImageLoaderArgs,
) -> Result<Option<DynamicImage>, anyhow::Error> {
	// If we're not downscaling, return
	if !args.downscale_load_from_cache && !args.downscale_save_to_cache {
		return Ok(None);
	}

	// Else load the downscale cache
	let downscale_cache = DownscaleCache::load(path, image_size).context("Unable to load the downscale cache")?;

	// Calculate the scroll direction and what kind of resize it needs for future operations
	let scroll_dir = ScrollDir::calculate(image_size, request.window_size);
	let resize = scroll_dir.resize(image_size, request.window_size);

	// If we're allowed to load from cache and we have an exact match, use it instead
	if args.downscale_load_from_cache {
		if let Some(cached_image) = downscale_cache.get_exact(request.window_size) {
			if let Some(value) = self::load_image_downscaled_cached(path, &cached_image, image_size) {
				return value;
			}
		}
	}

	// If we're allowed to save to the cache and the image should be downscaled, load the image, resize it and save it
	if args.downscale_save_to_cache {
		// If we're downscaling, downscale it and save it
		if let Some(Resize {
			size,
			kind: ResizeKind::Downscale,
		}) = resize
		{
			// Decode and resize
			let image = ImageReader::with_format(image_file, format)
				.decode()
				.context("Unable to decode image")?
				.resize_exact(size.x, size.y, FilterType::Lanczos3);

			// Then save it
			match downscale_cache.save(&image) {
				Ok(cached_image) => log::trace!(
					"Saved downscaled {path:?} ({}x{}) in {:?} ({}x{})",
					image_size.x,
					image_size.y,
					cached_image.path,
					cached_image.size.x,
					cached_image.size.y,
				),
				Err(err) => log::warn!("Unable to save downscaled {path:?}: {err:?}"),
			}

			return Ok(Some(image));
		}
	}

	// Else if we're allowed to load from the cache, load the nearest
	if args.downscale_load_from_cache {
		// If we got any smaller image that fits, load it
		if let Some(cached_image) = downscale_cache.get_smallest(request.window_size) {
			if let Some(value) = self::load_image_downscaled_cached(path, &cached_image, image_size) {
				return value;
			}
		}
	}

	Ok(None)
}

/// Loads an image downscaled from `cached_image`
fn load_image_downscaled_cached(
	path: &Path, cached_image: &downscale_cache::CachedImage, image_size: Vector2<u32>,
) -> Option<Result<Option<DynamicImage>, anyhow::Error>> {
	match image::open(&cached_image.path) {
		Ok(image) => {
			log::trace!(
				"Loaded downscaled {path:?} ({}x{}) from {:?} ({}x{})",
				image_size.x,
				image_size.y,
				cached_image.path,
				cached_image.size.x,
				cached_image.size.y,
			);
			return Some(Ok(Some(image)));
		},
		Err(err) => {
			log::warn!(
				"Unable to load downscaled {path:?} from {:?}: {err:?}",
				cached_image.path
			);
		},
	}
	None
}


/// Image scrolling direction
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum ScrollDir {
	Vertically,
	Horizontally,
	None,
}

impl ScrollDir {
	/// Selects the width or height, depending on which one is constant while scrolling.
	///
	/// If scrolling horizontally, chooses the height, else the width
	pub const fn select_constant_dir(self, size: Vector2<u32>) -> u32 {
		match self {
			Self::Vertically | Self::None => size.x,
			Self::Horizontally => size.y,
		}
	}

	/// Calculates if an image needs to be resized
	pub fn resize(self, image_size: Vector2<u32>, window_size: Vector2<u32>) -> Option<Resize> {
		match self {
			// If we're scrolling vertically or not scrolling, downscale if the image width is larger than the window
			// width, else upscale
			Self::Vertically | Self::None => Some(Resize {
				size: Vector2::new(window_size.x, (window_size.x * image_size.y) / image_size.x),
				kind: match image_size.x.cmp(&window_size.x) {
					Ordering::Less => ResizeKind::Upscale,
					Ordering::Equal => return None,
					Ordering::Greater => ResizeKind::Downscale,
				},
			}),

			// If we're scrolling horizontally, downscale if the image height is larger than the window height, else
			// upscale
			Self::Horizontally => Some(Resize {
				size: Vector2::new((window_size.y * image_size.x) / image_size.y, window_size.y),
				kind: match image_size.y.cmp(&window_size.y) {
					Ordering::Less => ResizeKind::Upscale,
					Ordering::Equal => return None,
					Ordering::Greater => ResizeKind::Downscale,
				},
			}),
		}
	}

	/// Calculates the scrolling direction of an image of size `image_size`
	/// in a window of size `window_size`
	pub fn calculate(image_size: Vector2<u32>, window_size: Vector2<u32>) -> Self {
		// Get it's width and aspect ratio
		let image_aspect_ratio = Ratio::new(image_size.x, image_size.y);
		let window_aspect_ratio = Ratio::new(window_size.x, window_size.y);

		// Then check what direction we'll be scrolling the image
		match (image_size.x.cmp(&image_size.y), window_size.x.cmp(&window_size.y)) {
			// If they're both square, no scrolling occurs
			(Ordering::Equal, Ordering::Equal) => Self::None,

			// Else if the image is tall and the window is wide, it must scroll vertically
			(Ordering::Less | Ordering::Equal, Ordering::Greater | Ordering::Equal) => Self::Vertically,

			// Else if the image is wide and the window is tall, it must scroll horizontally
			(Ordering::Greater | Ordering::Equal, Ordering::Less | Ordering::Equal) => Self::Horizontally,

			// Else we need to check the aspect ratio
			(Ordering::Less, Ordering::Less) | (Ordering::Greater, Ordering::Greater) => {
				match image_aspect_ratio.cmp(&window_aspect_ratio) {
					// If the image is wider than the screen, we'll scroll horizontally
					Ordering::Greater => Self::Horizontally,

					// Else if the image is taller than the screen, we'll scroll vertically
					Ordering::Less => Self::Vertically,

					// Else if they're equal, no scrolling occurs
					Ordering::Equal => Self::None,
				}
			},
		}
	}
}

/// Resize
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
struct Resize {
	/// Size
	size: Vector2<u32>,

	/// Kind
	kind: ResizeKind,
}

/// Resize kind
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum ResizeKind {
	/// Downscale
	Downscale,

	/// Upscale
	Upscale,
}
