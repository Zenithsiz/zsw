//! Args

// Imports
use crate::{img::ImageLoaderArgs, Rect};
use anyhow::Context;
use cgmath::{EuclideanSpace, Point2, Vector2};
use clap::{App as ClapApp, Arg as ClapArg};
use std::{fs, path::PathBuf, sync::Arc, time::Duration};

/// Args
#[derive(Debug)]
pub struct Args {
	/// Window geometry
	pub window_geometry: Rect<u32>,

	/// Image geometries
	pub image_geometries: Vec<Rect<u32>>,

	/// Image duration
	pub image_duration: Duration,

	/// Images directory
	pub images_dir: Arc<PathBuf>,

	/// Fade point (0.5..1.0)
	pub fade_point: f32,

	/// Image backlog
	pub image_backlog: Option<usize>,

	/// Image loader arguments
	pub image_loader_args: ImageLoaderArgs,
}

/// Parses all arguments
#[allow(clippy::too_many_lines)] // TODO: Refactor
pub fn get() -> Result<Args, anyhow::Error> {
	/// All arguments' names
	mod arg_name {
		pub const WINDOW_GEOMETRY: &str = "window-geometry";
		pub const IMAGE_GEOMETRY: &str = "image-geometry";
		pub const IMAGES_DIR: &str = "images-dir";
		pub const IMAGE_DURATION: &str = "image-duration";
		pub const FADE_POINT: &str = "fade-point";
		pub const GRID: &str = "grid";
		pub const IMAGE_BACKLOG: &str = "image-backlog";
		pub const IMAGE_LOADER_ARGS: &str = "image-loader-args";
	}


	// Get all matches from cli
	let matches = ClapApp::new("Zss")
		.version("1.0")
		.author("Filipe Rodrigues <filipejacintorodrigues1@gmail.com>")
		.about("Zenithsiz's scrolling wallpaper")
		.arg(
			ClapArg::with_name(arg_name::WINDOW_GEOMETRY)
				.help("Window geometry")
				.long_help("Window geometry (`{width}x{height}+{x}+{y}` or `{width}x{height}`)")
				.long_help(
					"Geometry to place the window in, this will typically be your full desktop.\nIf you use multiple \
					 monitors, set this to the full geometry and use image geometries to position all wallpapers",
				)
				.takes_value(true)
				.required(true)
				.long("window-geometry")
				.short("g"),
		)
		.arg(
			ClapArg::with_name(arg_name::IMAGE_GEOMETRY)
				.help("Window geometry")
				.long_help("Specifies a specific window geometry (`{width}x{height}+{x}+{y}` or `{width}x{height}`)")
				.long_help(
					"Used when you want to only display in a part of the window geometry.\nIf not specified, it is \
					 the window geometry.\nMultiple may be specified and they will all display images",
				)
				.takes_value(true)
				.multiple(true)
				.long("image-geometry"),
		)
		.arg(
			ClapArg::with_name(arg_name::GRID)
				.help("Adds a grid of image geometries (`{columns}x{rows}@{geometry}``)")
				.takes_value(true)
				.multiple(true)
				.long("grid"),
		)
		.arg(
			ClapArg::with_name(arg_name::IMAGES_DIR)
				.help("Images Directory")
				.long_help("Path to directory with images. Non-images will be ignored.")
				.takes_value(true)
				.required(true)
				.long("images-dir")
				.index(1),
		)
		.arg(
			ClapArg::with_name(arg_name::IMAGE_DURATION)
				.help("Duration (in seconds) of each image")
				.long_help("Duration, in seconds, each image will take up on screen, including during fading.")
				.takes_value(true)
				.long("image-duration")
				.default_value("30"),
		)
		.arg(
			ClapArg::with_name(arg_name::FADE_POINT)
				.help("Fade percentage (0.5 .. 1.0)")
				.long_help("Percentage, from 0.5 to 1.0, of when to start fading the image during it's display.")
				.takes_value(true)
				.long("fade-point")
				.default_value("0.8"),
		)
		.arg(
			ClapArg::with_name(arg_name::IMAGE_BACKLOG)
				.help("Image backlog per geometry")
				.long_help("Number of images to have in the backlog for each geometry")
				.takes_value(true)
				.long("image-backlog"),
		)
		.arg(
			ClapArg::with_name(arg_name::IMAGE_LOADER_ARGS)
				.help("Image loader arguments path")
				.long_help("File to load image loader arguments from")
				.takes_value(true)
				.long("image-loader-args"),
		)
		.get_matches();

	let window_geometry = matches
		.value_of(arg_name::WINDOW_GEOMETRY)
		.context("Argument with default value was missing")?;
	let window_geometry = Rect::parse_from_geometry(window_geometry).context("Unable to parse window geometry")?;

	// Get the specified image geometries, if any
	let mut image_geometries = matches
		.values_of(arg_name::IMAGE_GEOMETRY)
		.map_or_else(
			|| Ok(vec![]),
			|geometries| {
				geometries
					.map(|geometry| {
						Rect::parse_from_geometry(geometry).with_context(|| format!("Unable to parse {geometry}"))
					})
					.collect::<Result<Vec<_>, anyhow::Error>>()
			},
		)
		.context("Unable to parse image geometries")?;


	// Then add all geometries from the grids
	if let Some(grids) = matches.values_of(arg_name::GRID) {
		for grid in grids {
			// Split at the first `@`
			let (grid_size, geometry) = grid.split_once('@').context("Missing @ in grid string")?;

			// Split at the first `x` to get the columns and rows
			let (columns, rows) = grid_size.split_once('x').context("Unable to find `x` in size")?;

			let columns = columns.parse::<u32>().context("Unable to parse columns")?;
			let rows = rows.parse::<u32>().context("Unable to parse rows")?;

			let geometry = Rect::<u32>::parse_from_geometry(geometry).context("Unable to parse geometry")?;

			for column in 0..columns {
				for row in 0..rows {
					image_geometries.push(Rect {
						pos:  Point2::new(
							geometry.pos[0] + (column * geometry.size[0]) / columns,
							geometry.pos[1] + (row * geometry.size[1]) / rows,
						),
						size: Vector2::new(geometry.size[0] / columns, geometry.size[1] / rows),
					});
				}
			}
		}
	}

	// If there are no image geometries, add one with the window geometry (but without any offset)
	if image_geometries.is_empty() {
		image_geometries.push(Rect {
			pos:  Point2::origin(),
			size: window_geometry.size,
		});
	}


	let duration = matches
		.value_of(arg_name::IMAGE_DURATION)
		.context("Argument with default value was missing")?;
	let duration = duration.parse().context("Unable to parse duration")?;
	let image_duration = Duration::from_secs_f32(duration);

	let images_dir = PathBuf::from(
		matches
			.value_of_os(arg_name::IMAGES_DIR)
			.context("Required argument was missing")?,
	);
	let images_dir = Arc::new(images_dir);

	let fade = matches
		.value_of(arg_name::FADE_POINT)
		.context("Argument with default value was missing")?;
	let fade_point = fade.parse().context("Unable to parse fade")?;
	anyhow::ensure!((0.5..=1.0).contains(&fade_point), "Fade must be within 0.5 .. 1.0");

	let image_backlog = matches
		.value_of(arg_name::IMAGE_BACKLOG)
		.map(str::parse)
		.transpose()
		.context("Unable to parse image backlog")?;

	let image_loader_args = match matches.value_of_os(arg_name::IMAGE_LOADER_ARGS) {
		Some(path) => {
			let file = fs::File::open(path).context("Unable to open image loader args")?;
			serde_json::from_reader(file).context("Unable to parse image loader args")?
		},
		None => ImageLoaderArgs::default(),
	};

	Ok(Args {
		window_geometry,
		image_geometries,
		image_duration,
		images_dir,
		fade_point,
		image_backlog,
		image_loader_args,
	})
}
