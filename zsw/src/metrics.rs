//! Metrics

// Imports
use {
	crate::panel::state::fade::PanelFadeImageSlot,
	core::{ops::DerefMut, time::Duration},
	std::collections::HashMap,
	tokio::sync::{Mutex, MutexGuard},
	winit::window::WindowId,
	zsw_util::FrameTimes,
};

/// Window metrics
#[derive(Default, Debug)]
struct WindowMetrics {
	/// Render frame times
	render_frame_times: FrameTimes<RenderFrameTime>,

	/// Render panels frame times
	render_panels_frame_times: FrameTimes<RenderPanelsFrameTime>,
}

/// Metrics
#[derive(Debug)]
pub struct Metrics {
	/// Per-window metrics
	window_metrics: Mutex<HashMap<WindowId, WindowMetrics>>,
}

impl Metrics {
	/// Creates new, empty, metrics
	pub fn new() -> Self {
		Self {
			window_metrics: Mutex::new(HashMap::new()),
		}
	}

	/// Accesses render frame times metrics for a window
	pub async fn render_frame_times(&self, window_id: WindowId) -> impl DerefMut<Target = FrameTimes<RenderFrameTime>> {
		let window_metrics = self.window_metrics.lock().await;
		MutexGuard::map(window_metrics, |metrics| {
			&mut metrics.entry(window_id).or_default().render_frame_times
		})
	}

	/// Accesses render panel frame times metrics for a window
	pub async fn render_panels_frame_times(
		&self,
		window_id: WindowId,
	) -> impl DerefMut<Target = FrameTimes<RenderPanelsFrameTime>> {
		let window_metrics = self.window_metrics.lock().await;
		MutexGuard::map(window_metrics, |metrics| {
			&mut metrics.entry(window_id).or_default().render_panels_frame_times
		})
	}

	/// Returns all window ids from the metrics
	pub async fn window_ids<C>(&self) -> C
	where
		C: FromIterator<WindowId>,
	{
		self.window_metrics.lock().await.keys().copied().collect()
	}
}


/// Render frame time.
#[derive(Clone, Copy, Debug)]
pub struct RenderFrameTime {
	pub wait_next_frame: Duration,
	pub paint_egui:      Duration,
	pub render_start:    Duration,
	pub render_panels:   Duration,
	pub render_egui:     Duration,
	pub render_finish:   Duration,
	pub handle_events:   Duration,
}

/// Render panels frame time.
#[derive(Clone, Debug)]
pub struct RenderPanelsFrameTime {
	pub create_render_pass: Duration,
	pub lock_panels:        Duration,
	pub panels:             HashMap<usize, RenderPanelFrameTime>,
}

/// Render panel frame time.
#[derive(Clone, Debug)]
pub struct RenderPanelFrameTime {
	pub update_panel:           Duration,
	pub create_render_pipeline: Duration,
	pub geometries:             HashMap<usize, RenderPanelGeometryFrameTime>,
}

/// Render panel geometry frame time.
#[derive(Clone, Debug)]
#[derive(derive_more::From)]
pub enum RenderPanelGeometryFrameTime {
	None(RenderPanelGeometryNoneFrameTime),
	Fade(RenderPanelGeometryFadeFrameTime),
	Slide(RenderPanelGeometrySlideFrameTime),
}

/// Render panel geometry none frame time.
#[derive(Clone, Debug)]
pub struct RenderPanelGeometryNoneFrameTime {
	pub write_uniforms: Duration,
	pub draw:           Duration,
}

/// Render panel geometry fade frame time.
#[derive(Clone, Debug)]
pub struct RenderPanelGeometryFadeFrameTime {
	pub images: HashMap<PanelFadeImageSlot, RenderPanelGeometryFadeImageFrameTime>,
}

/// Render panel geometry fade image frame time.
#[derive(Clone, Debug)]
pub struct RenderPanelGeometryFadeImageFrameTime {
	pub write_uniforms: Duration,
	pub draw:           Duration,
}

/// Render panel geometry slide frame time.
#[derive(Clone, Debug)]
pub struct RenderPanelGeometrySlideFrameTime {
	pub write_uniforms: Duration,
	pub draw:           Duration,
}
