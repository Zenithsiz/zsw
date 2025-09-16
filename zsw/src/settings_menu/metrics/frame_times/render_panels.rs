//! Render panels frame time metric

// Imports
use {
	crate::metrics::{FrameTimes, RenderPanelFrameTime, RenderPanelGeometryFrameTime, RenderPanelsFrameTime},
	core::{iter, time::Duration},
	itertools::Itertools,
	std::collections::{HashMap, HashSet},
};

/// Draws the render panel frame times
pub fn draw(ui: &mut egui::Ui, render_frame_times: &mut FrameTimes<RenderPanelsFrameTime>) {
	let settings = super::draw_frame_time_settings(ui, render_frame_times);

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
					[DurationPanelGeometryIdx::WriteUniforms, DurationPanelGeometryIdx::Draw]
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
		let chart = super::add_frame_time_chart(
			render_frame_times,
			settings.is_histogram,
			settings.histogram_time_scale,
			settings.stack_charts,
			&charts,
			&duration_idx,
		);
		charts.push(chart);
	}

	super::draw_plot(ui, &settings, charts);
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
	WriteUniforms,
	Draw,
}

impl DurationPanelGeometryIdx {
	pub fn name(self, panel_idx: usize, geometry_idx: usize) -> String {
		match self {
			Self::WriteUniforms => format!("[Panel${panel_idx}] [Geometry${geometry_idx}] Write uniforms"),
			Self::Draw => format!("[Panel${panel_idx}] [Geometry${geometry_idx}] Draw"),
		}
	}

	pub fn duration(self, frame_time: &RenderPanelGeometryFrameTime) -> Option<Duration> {
		match self {
			Self::WriteUniforms => Some(frame_time.write_uniforms),
			Self::Draw => Some(frame_time.draw),
		}
	}
}
