//! Arguments

// Imports
use crate::Rect;
use anyhow::Context;
use cgmath::{EuclideanSpace, Point2, Vector2};
use clap::{App as ClapApp, Arg as ClapArg};
use std::{path::PathBuf, sync::Arc, time::Duration};

/// Arguments
#[derive(Debug)]
pub struct Args {
	/// Window geometry
	pub window_geometry: Rect<u32>,

	/// Panel geometries
	pub panel_geometries: Vec<Rect<u32>>,

	/// Image duration
	pub image_duration: Duration,

	/// Images directory
	pub images_dir: Arc<PathBuf>,

	/// Fade point (0.5..1.0)
	pub fade_point: f32,

	/// Image backlog
	pub image_backlog: Option<usize>,
}

/// Parses all arguments
#[allow(clippy::too_many_lines)] // TODO: Refactor
pub fn get() -> Result<Args, anyhow::Error> {
	/// All arguments' names
	mod arg_name {
		pub const WINDOW_GEOMETRY: &str = "window-geometry";
		pub const PANEL_GEOMETRY: &str = "panel-geometry";
		pub const IMAGES_DIR: &str = "images-dir";
		pub const IMAGE_DURATION: &str = "image-duration";
		pub const FADE_POINT: &str = "fade-point";
		pub const GRID: &str = "grid";
		pub const IMAGE_BACKLOG: &str = "image-backlog";
	}

	// Get all matches from cli
	let matches = ClapApp::new("Zss")
		.version("1.0")
		.author("Filipe Rodrigues <filipejacintorodrigues1@gmail.com>")
		.about("Zenithsiz's scrolling wallpaper")
		.arg(
			ClapArg::new(arg_name::WINDOW_GEOMETRY)
				.help("Window geometry")
				.long_help("Window geometry (`{width}x{height}+{x}+{y}` or `{width}x{height}`)")
				.long_help(concat!(
					"Geometry to place the window at, consisting of either width and height (`{width}x{height}`), or \
					 width, height and an offset (`{width}x{height}+{x}+{y}`).\n",
					"This will typically be your full desktop size, including if you use multiple monitors.\n",
					"Using panels, you may populate sub-geometries of this window geometry to display images in"
				))
				.takes_value(true)
				.required(true)
				.long("window-geometry")
				.short('g'),
		)
		.arg(
			ClapArg::new(arg_name::PANEL_GEOMETRY)
				.help("Panel geometry")
				.long_help("Panel geometry (`{width}x{height}+{x}+{y}` or `{width}x{height}`)")
				.long_help(concat!(
					"Used when you want to render images in a sub-geometry of the window geometry.\n",
					"Each panel will render images independently. If none are given, defaults to a single panel \
					 covering the whole window geometry"
				))
				.takes_value(true)
				.multiple_occurrences(true)
				.long("panel-geometry")
				.short('p'),
		)
		.arg(
			ClapArg::new(arg_name::GRID)
				.help("Grid of panel geometries (`{columns}x{rows}@{geometry}``)")
				.takes_value(true)
				.multiple_occurrences(true)
				.long("grid"),
		)
		.arg(
			ClapArg::new(arg_name::IMAGES_DIR)
				.help("Images Directory")
				.long_help("Path to directory with images. Non-images will be ignored.")
				.takes_value(true)
				.required(true)
				.allow_invalid_utf8(true)
				.index(1),
		)
		.arg(
			ClapArg::new(arg_name::IMAGE_DURATION)
				.help("Duration (in seconds) of each image")
				.long_help("Duration, in seconds, each image will take up on screen, including during fading.")
				.takes_value(true)
				.long("image-duration")
				.default_value("30"),
		)
		.arg(
			ClapArg::new(arg_name::FADE_POINT)
				.help("Fade percentage (0.5 .. 1.0)")
				.long_help("Percentage, from 0.5 to 1.0, of when to start fading the image during it's display.")
				.takes_value(true)
				.long("fade-point")
				.default_value("0.8"),
		)
		.arg(
			ClapArg::new(arg_name::IMAGE_BACKLOG)
				.help("Image backlog per geometry")
				.long_help(
					"Number of images to have in the backlog for each geometry. Will be the number of threads by \
					 default",
				)
				.takes_value(true)
				.long("image-backlog"),
		)
		.get_matches();

	let window_geometry = matches
		.value_of(arg_name::WINDOW_GEOMETRY)
		.context("Argument with default value was missing")?;
	let window_geometry = Rect::parse_from_geometry(window_geometry).context("Unable to parse window geometry")?;

	// Get the specified image geometries, if any
	let mut image_geometries = matches
		.values_of(arg_name::PANEL_GEOMETRY)
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

	Ok(Args {
		window_geometry,
		panel_geometries: image_geometries,
		image_duration,
		images_dir,
		fade_point,
		image_backlog,
	})
}
