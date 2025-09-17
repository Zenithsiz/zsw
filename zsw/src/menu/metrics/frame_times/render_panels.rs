//! Render panels frame time metric

// Imports
use {
	crate::metrics::{
		FrameTimes,
		RenderPanelFrameTime,
		RenderPanelGeometryFadeFrameTime,
		RenderPanelGeometryFrameTime,
		RenderPanelGeometryNoneFrameTime,
		RenderPanelGeometrySlideFrameTime,
		RenderPanelsFrameTime,
	},
	core::{iter, time::Duration},
	itertools::Itertools,
	std::collections::{HashMap, HashSet},
	zsw_util::iter_chain,
};

/// Draws the render panel frame times
pub fn draw(ui: &mut egui::Ui, render_frame_times: &mut FrameTimes<RenderPanelsFrameTime>) {
	let display = super::draw_display_settings(ui, render_frame_times);

	// Go through all frame times and record which panels and geometries we saw
	// so we can create the correct indices after.
	struct DurationIdxTree {
		panels: HashMap<usize, DurationPanelIdxTree>,
	}
	struct DurationPanelIdxTree {
		geometries: HashSet<usize>,
	}
	let duration_idxs_tree = DurationIdxTree {
		panels: render_frame_times
			.iter()
			.flat_map(|frame_time| {
				frame_time.panels.iter().map(|(&panel_idx, panel)| {
					let panel = DurationPanelIdxTree {
						geometries: panel.geometries.keys().copied().collect(),
					};
					(panel_idx, panel)
				})
			})
			.collect(),
	};

	// Create all duration indices based on all of the panels and geometries we saw.
	let duration_idxs = iter::chain(
		// Non-panel specific indices
		[DurationIdx::CreateRenderPass, DurationIdx::LockPanels],
		duration_idxs_tree.panels.into_iter().flat_map(|(panel_idx, panel)| {
			iter::chain(
				// Panel specific indices
				[DurationPanelIdx::UpdatePanel, DurationPanelIdx::CreateRenderPipeline],
				panel.geometries.into_iter().flat_map(|geometry_idx| {
					// Panel & geometry specific indices
					iter_chain!(
						[
							DurationPanelGeometryNoneIdx::WriteUniforms,
							DurationPanelGeometryNoneIdx::Draw,
						]
						.map(move |inner| DurationPanelGeometryIdx::None { inner }),
						[
							DurationPanelGeometryFadeIdx::WriteUniforms,
							DurationPanelGeometryFadeIdx::Draw,
						]
						.map(move |inner| DurationPanelGeometryIdx::Fade { inner }),
						[
							DurationPanelGeometrySlideIdx::WriteUniforms,
							DurationPanelGeometrySlideIdx::Draw,
						]
						.map(move |inner| DurationPanelGeometryIdx::Slide { inner }),
					)
					.map(move |inner| DurationPanelIdx::Geometries { geometry_idx, inner })
				}),
			)
			.map(move |inner| DurationIdx::Panels { panel_idx, inner })
		}),
	)
	.sorted()
	.collect::<Vec<_>>();

	let mut charts = vec![];
	for duration_idx in duration_idxs {
		let chart = super::add_frame_time_chart(render_frame_times, &display, &charts, &duration_idx);
		charts.push(chart);
	}

	super::draw_plot(ui, &display, charts);
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, Debug)]
enum DurationIdx {
	CreateRenderPass,
	LockPanels,
	Panels {
		panel_idx: usize,
		inner:     DurationPanelIdx,
	},
}

impl super::DurationIdx<RenderPanelsFrameTime> for DurationIdx {
	fn name(&self) -> String {
		match self {
			Self::CreateRenderPass => "Create render pass".to_owned(),
			Self::LockPanels => "Lock panels".to_owned(),
			Self::Panels { panel_idx, inner } => inner.name(*panel_idx),
		}
	}

	fn duration_of(&self, frame_time: &RenderPanelsFrameTime) -> Option<Duration> {
		match self {
			Self::CreateRenderPass => Some(frame_time.create_render_pass),
			Self::LockPanels => Some(frame_time.lock_panels),
			Self::Panels { panel_idx, inner } => frame_time
				.panels
				.get(panel_idx)
				.and_then(|frame_times| inner.duration(frame_times)),
		}
	}
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, Debug)]
enum DurationPanelIdx {
	UpdatePanel,
	CreateRenderPipeline,
	Geometries {
		geometry_idx: usize,
		inner:        DurationPanelGeometryIdx,
	},
}

impl DurationPanelIdx {
	pub fn name(self, panel_idx: usize) -> String {
		match self {
			Self::UpdatePanel => format!("[Panel${panel_idx}] Update panel"),
			Self::CreateRenderPipeline => format!("[Panel${panel_idx}] Create render pipeline"),
			Self::Geometries { geometry_idx, inner } => inner.name(panel_idx, geometry_idx),
		}
	}

	pub fn duration(self, frame_time: &RenderPanelFrameTime) -> Option<Duration> {
		match self {
			Self::UpdatePanel => Some(frame_time.update_panel),
			Self::CreateRenderPipeline => Some(frame_time.create_render_pipeline),
			Self::Geometries { geometry_idx, inner } => frame_time
				.geometries
				.get(&geometry_idx)
				.and_then(|frame_time| inner.duration(frame_time)),
		}
	}
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, Debug)]
enum DurationPanelGeometryIdx {
	None { inner: DurationPanelGeometryNoneIdx },
	Fade { inner: DurationPanelGeometryFadeIdx },
	Slide { inner: DurationPanelGeometrySlideIdx },
}

impl DurationPanelGeometryIdx {
	pub fn name(self, panel_idx: usize, geometry_idx: usize) -> String {
		match self {
			Self::None { inner } => inner.name(panel_idx, geometry_idx),
			Self::Fade { inner } => inner.name(panel_idx, geometry_idx),
			Self::Slide { inner } => inner.name(panel_idx, geometry_idx),
		}
	}

	pub fn duration(self, frame_time: &RenderPanelGeometryFrameTime) -> Option<Duration> {
		match (self, frame_time) {
			(Self::None { inner }, RenderPanelGeometryFrameTime::None(frame_time)) => inner.duration(frame_time),
			(Self::Fade { inner }, RenderPanelGeometryFrameTime::Fade(frame_time)) => inner.duration(frame_time),
			(Self::Slide { inner }, RenderPanelGeometryFrameTime::Slide(frame_time)) => inner.duration(frame_time),
			_ => None,
		}
	}
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, Debug)]
enum DurationPanelGeometryNoneIdx {
	WriteUniforms,
	Draw,
}

impl DurationPanelGeometryNoneIdx {
	pub fn name(self, panel_idx: usize, geometry_idx: usize) -> String {
		match self {
			Self::WriteUniforms =>
				format!("[Panel${panel_idx}] [Geometry${geometry_idx}] [Shader$None] Write uniforms"),
			Self::Draw => format!("[Panel${panel_idx}] [Geometry${geometry_idx}] [Shader$None] Draw"),
		}
	}

	pub fn duration(self, frame_time: &RenderPanelGeometryNoneFrameTime) -> Option<Duration> {
		match self {
			Self::WriteUniforms => Some(frame_time.write_uniforms),
			Self::Draw => Some(frame_time.draw),
		}
	}
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, Debug)]
enum DurationPanelGeometryFadeIdx {
	WriteUniforms,
	Draw,
}

impl DurationPanelGeometryFadeIdx {
	pub fn name(self, panel_idx: usize, geometry_idx: usize) -> String {
		match self {
			Self::WriteUniforms =>
				format!("[Panel${panel_idx}] [Geometry${geometry_idx}] [Shader$Fade] Write uniforms"),
			Self::Draw => format!("[Panel${panel_idx}] [Geometry${geometry_idx}] [Shader$Fade] Draw"),
		}
	}

	pub fn duration(self, frame_time: &RenderPanelGeometryFadeFrameTime) -> Option<Duration> {
		match self {
			Self::WriteUniforms => Some(frame_time.write_uniforms),
			Self::Draw => Some(frame_time.draw),
		}
	}
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, Debug)]
enum DurationPanelGeometrySlideIdx {
	WriteUniforms,
	Draw,
}

impl DurationPanelGeometrySlideIdx {
	pub fn name(self, panel_idx: usize, geometry_idx: usize) -> String {
		match self {
			Self::WriteUniforms =>
				format!("[Panel${panel_idx}] [Geometry${geometry_idx}] [Shader$Slide] Write uniforms"),
			Self::Draw => format!("[Panel${panel_idx}] [Geometry${geometry_idx}] [Shader$Slide] Draw"),
		}
	}

	pub fn duration(self, frame_time: &RenderPanelGeometrySlideFrameTime) -> Option<Duration> {
		match self {
			Self::WriteUniforms => Some(frame_time.write_uniforms),
			Self::Draw => Some(frame_time.draw),
		}
	}
}
