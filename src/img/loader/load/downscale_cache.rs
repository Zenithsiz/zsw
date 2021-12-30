//! Downscale cache

// Imports
use super::ScrollDir;
use crate::util;
use anyhow::Context;
use cgmath::Vector2;
use image::{DynamicImage, GenericImageView};
use std::{
	ffi::OsString,
	fs, io,
	path::{Path, PathBuf},
};

/// Base path of cache
const CACHE_PATH: &str = "/home/filipe/.cache/zsw/downscale-cache";

/// Downscale cache
#[derive(PartialEq, Clone, Debug)]
pub struct DownscaleCache<'a> {
	/// Path
	path: &'a Path,

	/// Image base size
	image_size: Vector2<u32>,

	/// Cache path
	cache_path: PathBuf,

	/// All entries
	entries: Vec<CacheEntry>,
}

impl<'a> DownscaleCache<'a> {
	/// Loads the downscale scale for an image
	pub fn load(path: &'a Path, image_size: Vector2<u32>) -> Result<Self, anyhow::Error> {
		// Get the path hash
		let path_hash = util::hash_of(path);

		// And build the cache path with it
		let cache_path = Path::new(CACHE_PATH).join(format!("{:016x}", path_hash));

		// Then get all entries
		let entries = match fs::read_dir(&cache_path) {
			Ok(entries) => entries
				.filter_map(|entry| match entry {
					Ok(entry) => match CacheEntry::parse(&entry) {
						Ok(entry) => Some(entry),
						Err(err) => {
							log::warn!("Unable to parse cache entry {entry:?} in {cache_path:?}: {err:?}");
							None
						},
					},
					Err(err) => {
						log::warn!("Skipping invalid entry in cache {cache_path:?}: {err:?}");
						None
					},
				})
				.collect::<Vec<_>>(),
			// If the cache doesn't exist, we have 0 entries
			Err(err) if err.kind() == io::ErrorKind::NotFound => vec![],
			Err(err) => Err(err).with_context(|| format!("Unable to read cache directory {cache_path:?}"))?,
		};

		Ok(Self {
			path,
			image_size,
			cache_path,
			entries,
		})
	}

	/// Gets an exact match for a cached image, if it exists
	pub fn get_exact(&self, window_size: Vector2<u32>) -> Option<CachedImage> {
		let scroll_dir = ScrollDir::calculate(self.image_size, window_size);
		self.entries
			.iter()
			.find(|entry| scroll_dir.select_constant_dir(entry.size) == scroll_dir.select_constant_dir(window_size))
			.map(|entry| CachedImage {
				path: self.cache_path.join(&entry.file_name),
				size: entry.size,
			})
	}

	/// Gets the smallest cached image that fits in `window_size`
	pub fn get_smallest(&self, window_size: Vector2<u32>) -> Option<CachedImage> {
		// Get the smallest match above the window size, depending on scroll direction
		let scroll_dir = ScrollDir::calculate(self.image_size, window_size);
		self.entries
			.iter()
			.filter(|entry| scroll_dir.select_constant_dir(entry.size) >= scroll_dir.select_constant_dir(window_size))
			.min_by_key(|entry| scroll_dir.select_constant_dir(entry.size))
			.map(|entry| CachedImage {
				path: self.cache_path.join(&entry.file_name),
				size: entry.size,
			})
	}

	/// Saves an image to the cache and returns it
	pub fn save(&self, image: &DynamicImage) -> Result<CachedImage, anyhow::Error> {
		// Get the extension and image size
		let _extension = self.path.extension().context("Image had no extension")?;
		let image_size = Vector2::new(image.width(), image.height());

		// Then build the path of the cached image
		// Note: We always use png for downscaled images to prevent jpeg artifacts
		//       from taking over at such small sizes.
		let image_path = self
			.cache_path
			.join(format!("{}x{}", image_size.x, image_size.y))
			.with_extension("png");

		// Finally save it after making sure the cache directory exists
		fs::create_dir_all(&self.cache_path)
			.with_context(|| format!("Unable to create cache directory {:?}", self.cache_path))?;
		image
			.save(&image_path)
			.with_context(|| format!("Unable to save image in cache path {image_path:?}"))?;

		Ok(CachedImage {
			path: image_path,
			size: image_size,
		})
	}
}

/// Cache entry
#[derive(PartialEq, Clone, Debug)]
struct CacheEntry {
	/// Size
	size: Vector2<u32>,

	/// File name
	file_name: OsString,
}

impl CacheEntry {
	/// Parses a cache entry
	pub fn parse(entry: &fs::DirEntry) -> Result<Self, anyhow::Error> {
		// Make sure it's not a directory
		let file_type = entry.file_type().context("Unable to get file type")?;
		anyhow::ensure!(!file_type.is_dir(), "Entry cannot be a directory");

		// Then parse it's size
		let file_name = entry.file_name();
		let file_stem = Path::new(&file_name)
			.file_stem()
			.context("File has no stem")?
			.to_str()
			.context("File stem contained invalid utf-8")?;
		let (width, height) = file_stem.split_once("x").context("File stem missing 'x'")?;
		let width = width.parse().context("Unable to parse file width")?;
		let height = height.parse().context("Unable to parse file height")?;
		let size = Vector2::new(width, height);


		Ok(Self { size, file_name })
	}
}

/// A cached image
#[derive(PartialEq, Clone, Debug)]
pub struct CachedImage {
	/// Path
	pub path: PathBuf,

	/// Size
	pub size: Vector2<u32>,
}
