//! Image loader

// Imports
use crate::ImageBuffer;
use anyhow::Context;
use image::{imageops::FilterType, DynamicImage, GenericImageView};
use num_rational::Ratio;
use parking_lot::Mutex;
use std::{
	cmp::Ordering,
	collections::hash_map::DefaultHasher,
	ffi::OsStr,
	hash::{Hash, Hasher},
	path::Path,
	process::{self, Stdio},
	time::Instant,
};

/// Loads an image from a path given the window it's going to be displayed in
pub fn load_image(path: &Path, [window_width, window_height]: [u32; 2]) -> Result<ImageBuffer, anyhow::Error> {
	// Try to open the image by guessing it's format
	let image_reader = image::io::Reader::open(&path)
		.context("Unable to open image")?
		.with_guessed_format()
		.context("Unable to parse image")?;
	let image = image_reader.decode().context("Unable to decode image")?;

	// Get it's width and aspect ratio
	let (image_width, image_height) = (image.width(), image.height());
	let image_aspect_ratio = Ratio::new(image_width, image_height);
	let window_aspect_ratio = Ratio::new(window_width, window_height);

	log::trace!("Loaded {path:?} ({image_width}x{image_height})");

	// Then check what direction we'll be scrolling the image
	let scroll_dir = match (image_width.cmp(&image_height), window_width.cmp(&window_height)) {
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
	log::trace!("Scrolling image with directory: {scroll_dir:?}");

	// Then get the size we'll be resizing to, if any
	log::trace!("{scroll_dir:?} - {image_width:?}x{image_height:?} -> {window_width:?}x{window_height:?}");
	let resize = match scroll_dir {
		// If we're scrolling vertically, downscale if the image width is larger than the window width, else upscale
		ScrollDir::Vertically => Resize {
			width:  window_width,
			height: (window_width * image_height) / image_width,
			kind:   if image_width > window_width {
				ResizeKind::Downscale
			} else {
				ResizeKind::Upscale
			},
		},

		// If we're scrolling horizontally, downscale if the image height is larger than the window height, else upscale
		ScrollDir::Horizontally => Resize {
			width:  (window_height * image_width) / image_height,
			height: window_height,
			kind:   if image_height > window_height {
				ResizeKind::Downscale
			} else {
				ResizeKind::Upscale
			},
		},

		// If we're not doing any scrolling and the window is smaller, downscale the image to screen size, else upscale
		// Note: Since we're not scrolling, we know aspect ratio is the same and so
		//       we only need to check the width.
		ScrollDir::None => Resize {
			width:  window_width,
			height: window_height,
			kind:   if image_width > window_width {
				ResizeKind::Downscale
			} else {
				ResizeKind::Upscale
			},
		},
	};

	// And resize if necessary
	let resize_width = resize.width;
	let resize_height = resize.height;
	let resize_scale = 100.0 * (f64::from(resize_width) * f64::from(resize_height)) /
		(f64::from(image_width) * f64::from(image_height));
	let image = match resize.kind {
		ResizeKind::Downscale => {
			log::trace!(
				"Downscaling from {image_width}x{image_height} to {resize_width}x{resize_height} ({resize_scale:.2}%)",
			);
			image.resize_exact(resize_width, resize_height, FilterType::CatmullRom)
		},
		ResizeKind::Upscale => {
			log::trace!(
				"Upscaling from {image_width}x{image_height} to {resize_width}x{resize_height} ({resize_scale:.2}%)",
			);
			self::upscale(path, &image, resize_width, resize_height, scroll_dir).context("Unable to upscale image")?
		},
	};

	let image = image.flipv().to_rgba8();
	Ok(image)
}

/// Resize
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
struct Resize {
	/// Width
	width: u32,

	/// Height
	height: u32,

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
	path: &Path, image: &DynamicImage, resize_width: u32, resize_height: u32, scroll_dir: ScrollDir,
) -> Result<DynamicImage, anyhow::Error> {
	/// Mutex for only upscaling one image at a time
	static UPSCALE_MUTEX: Mutex<()> = parking_lot::const_mutex(());

	// Get the mutex
	let _guard = UPSCALE_MUTEX.lock();

	// Select which upscale fits best
	let (image_width, image_height) = (image.width(), image.height());
	let scale = match scroll_dir {
		ScrollDir::Vertically | ScrollDir::None => f64::from(resize_width) / f64::from(image_width),
		ScrollDir::Horizontally => f64::from(resize_height) / f64::from(image_height),
	};
	let scale = match scale {
		scale if scale <= 1.0 => unreachable!("Should be upscaling"),
		scale if scale <= 2.0 => "2",
		scale if scale <= 4.0 => "4",
		scale if scale <= 8.0 => "8",
		scale if scale <= 16.0 => "16",
		scale if scale <= 132.0 => "32",
		scale => anyhow::bail!("Unsupported resize scale: {}", scale),
	};

	// Get the hash of the image to make sure it's the same image
	let image_hash = {
		let mut hasher = DefaultHasher::new();
		image.hash(&mut hasher);

		hasher.finish()
	};

	// Create the output path
	// TODO: Use a proper cache for this instead of a hardcoded path
	// TODO: Not use png if we can't find it and instead use the same file type as `image`?
	let path_ext = path.extension().and_then(OsStr::to_str).unwrap_or("png");
	let output_path = format!("/home/filipe/.cache/zsw/upscale/{image_hash}-upscaled-{scale}.{path_ext}");
	let output_path = Path::new(&output_path);

	// If the output path doesn't exist, create it
	if !output_path.exists() {
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
			"Took {:.2}s to upscale {path:?}",
			Instant::now().duration_since(start_time).as_secs_f64()
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
		.resize_exact(resize_width, resize_height, FilterType::CatmullRom))
}
