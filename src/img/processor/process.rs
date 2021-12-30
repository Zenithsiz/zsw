//! Image processing

//! TODO: Adapt all of this onto the `load` module.

// Imports
use crate::{img::ImageRequest, ProcessedImage};
use anyhow::Context;
use cgmath::Vector2;
use image::{imageops::FilterType, DynamicImage, GenericImageView};
use num_rational::Ratio;
use parking_lot::Mutex;
use std::{
	cmp::Ordering,
	ffi::OsStr,
	hash::{Hash, Hasher},
	path::Path,
	process::{self, Stdio},
	time::Instant,
};

/// Processes an image according to a request
pub fn process_image(
	path: &Path, image: DynamicImage, request: ImageRequest, upscale: bool, downscale: bool, upscale_waifu2x: bool,
) -> Result<ProcessedImage, anyhow::Error> {
	let ImageRequest { window_size } = request;

	// Get it's width and aspect ratio
	let image_size = Vector2::new(image.width(), image.height());
	let image_aspect_ratio = Ratio::new(image_size.x, image_size.y);
	let window_aspect_ratio = Ratio::new(window_size.x, window_size.y);

	// Then check what direction we'll be scrolling the image
	let scroll_dir = match (image_size.x.cmp(&image_size.y), window_size.x.cmp(&window_size.y)) {
		// If they're both square, no scrolling occurs
		(Ordering::Equal, Ordering::Equal) => ScrollDir::None,

		// Else if the image is tall and the window is wide, it must scroll vertically
		(Ordering::Less | Ordering::Equal, Ordering::Greater | Ordering::Equal) => ScrollDir::Vertically,

		// Else if the image is wide and the window is tall, it must scroll horizontally
		(Ordering::Greater | Ordering::Equal, Ordering::Less | Ordering::Equal) => ScrollDir::Horizontally,

		// Else we need to check the aspect ratio
		(Ordering::Less, Ordering::Less) | (Ordering::Greater, Ordering::Greater) => {
			match image_aspect_ratio.cmp(&window_aspect_ratio) {
				// If the image is wider than the screen, we'll scroll horizontally
				Ordering::Greater => ScrollDir::Horizontally,

				// Else if the image is taller than the screen, we'll scroll vertically
				Ordering::Less => ScrollDir::Vertically,

				// Else if they're equal, no scrolling occurs
				Ordering::Equal => ScrollDir::None,
			}
		},
	};

	// Then get the size we'll be resizing to, if any
	let resize = match scroll_dir {
		// If we're scrolling vertically, downscale if the image width is larger than the window width, else upscale
		ScrollDir::Vertically => Resize {
			size: Vector2::new(window_size.x, (window_size.x * image_size.y) / image_size.x),
			kind: if image_size.x >= window_size.x {
				ResizeKind::Downscale
			} else {
				ResizeKind::Upscale
			},
		},

		// If we're scrolling horizontally, downscale if the image height is larger than the window height, else upscale
		ScrollDir::Horizontally => Resize {
			size: Vector2::new((window_size.y * image_size.x) / image_size.y, window_size.y),
			kind: if image_size.y >= window_size.y {
				ResizeKind::Downscale
			} else {
				ResizeKind::Upscale
			},
		},

		// If we're not doing any scrolling and the window is smaller, downscale the image to screen size, else upscale
		// Note: Since we're not scrolling, we know aspect ratio is the same and so
		//       we only need to check the width.
		ScrollDir::None => Resize {
			size: Vector2::new(window_size.x, window_size.y),
			kind: if image_size.x >= window_size.x {
				ResizeKind::Downscale
			} else {
				ResizeKind::Upscale
			},
		},
	};

	// And resize if necessary
	let resize_scale = 100.0 * (f64::from(resize.size.x) * f64::from(resize.size.y)) /
		(f64::from(image_size.x) * f64::from(image_size.y));
	let image = match resize.kind {
		ResizeKind::Downscale if downscale => {
			log::trace!(
				"Downscaling {path:?} from {}x{} to {}x{} ({resize_scale:.2}%)",
				image_size.x,
				image_size.y,
				resize.size.x,
				resize.size.y,
			);
			image.resize_exact(resize.size.x, resize.size.y, FilterType::CatmullRom)
		},
		ResizeKind::Upscale if upscale => {
			log::trace!(
				"Upscaling {path:?} from {}x{} to {}x{} ({resize_scale:.2}%)",
				image_size.x,
				image_size.y,
				resize.size.x,
				resize.size.y,
			);
			self::upscale(path, image, resize.size, scroll_dir, upscale_waifu2x).context("Unable to upscale image")?
		},
		_ => image,
	};

	let image = image.to_rgba8();
	Ok(image)
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

/// Image scrolling direction
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum ScrollDir {
	Vertically,
	Horizontally,
	None,
}

/// Upscales an image
fn upscale(
	path: &Path, image: DynamicImage, resize_size: Vector2<u32>, scroll_dir: ScrollDir, use_waifu2x: bool,
) -> Result<DynamicImage, anyhow::Error> {
	// Select which upscale fits best
	let image_size = Vector2::new(image.width(), image.height());
	let scale = match scroll_dir {
		ScrollDir::Vertically | ScrollDir::None => f64::from(resize_size.x) / f64::from(image_size.x),
		ScrollDir::Horizontally => f64::from(resize_size.y) / f64::from(image_size.y),
	};
	let scale = match scale {
		scale if scale <= 1.0 => unreachable!("Should be upscaling {:?}, found scale of {:?}", path, scale),
		scale if scale <= 2.0 => "2",
		scale if scale <= 4.0 => "4",
		scale if scale <= 8.0 => "8",
		scale if scale <= 16.0 => "16",
		scale if scale <= 132.0 => "32",
		scale => anyhow::bail!("Unsupported resize scale: {}", scale),
	};

	// Get the hash of the image to make sure it's the same image
	let image_hash = {
		let mut hasher = twox_hash::XxHash64::with_seed(0);
		image.hash(&mut hasher);

		hasher.finish()
	};

	// Create the output path
	// TODO: Use a proper cache for this instead of a hardcoded path
	// TODO: Not use png if we can't find it and instead use the same file type as `image`?
	let path_ext = path.extension().and_then(OsStr::to_str).unwrap_or("png");
	let output_path = format!("/home/filipe/.cache/zsw/upscale/{image_hash:016x}-upscaled-{scale}x.{path_ext}");
	let output_path = Path::new(&output_path);

	// If the output path doesn't exist, create it
	if !output_path.exists() {
		// If we shouldn't use waifu2x, return the original image
		if !use_waifu2x {
			return Ok(image);
		}

		// Get the mutex
		let _guard = UPSCALE_MUTEX.lock();

		// Else boot up `waifu2x` to do it
		// TODO: Use proper commands instead of just the ones that don't lag my computer to hell
		log::trace!("Starting upscale of {path:?} to {output_path:?} (x{scale})");

		let start_time = Instant::now();
		let mut proc = process::Command::new("waifu2x-ncnn-vulkan")
			.arg("-i")
			.arg(path)
			.arg("-o")
			.arg(&output_path)
			.arg("-s")
			.arg(scale)
			.args(&["-j", "1:1:1", "-t", "32"])
			.stdout(Stdio::null())
			.stderr(Stdio::null())
			.spawn()
			.context("Unable to run the `waifu2x-ncnn-vulkan` to upscale")?;
		proc.wait().context("`waifu2x-ncnn-vulkan` returned an error status")?;

		log::trace!(
			"Took {:.2?} to upscale {path:?}",
			Instant::now().duration_since(start_time)
		);
	} else {
		log::trace!("Upscaled version of {path:?} already exists at {output_path:?}");
	}

	// Then load it
	let image_reader = image::io::Reader::open(&output_path)
		.context("Unable to open upscaled image")?
		.with_guessed_format()
		.context("Unable to parse upscaled image")?;
	Ok(image_reader
		.decode()
		.context("Unable to decode upscaled image")?
		.resize_exact(resize_size.x, resize_size.y, FilterType::CatmullRom))
}

/// Mutex for only upscaling one image at a time
static UPSCALE_MUTEX: Mutex<()> = parking_lot::const_mutex(());
