//! Metrics

// Imports
use {
	core::time::Duration,
	std::{
		collections::{BTreeMap, HashMap, VecDeque},
		sync::nonpoison::Mutex,
	},
	winit::window::WindowId,
};

/// Window metrics
#[derive(Default, Debug)]
struct WindowMetrics {
	/// Render frame times
	render_frame_times: RenderFrameTimes,
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

	/// Adds a frame time to the metrics
	pub fn render_frame_times_add(&self, window_id: WindowId, frame_time: RenderFrameTime) {
		let mut window_metrics = self.window_metrics.lock();
		let window_metrics = window_metrics.entry(window_id).or_default();
		let frame_times = &mut window_metrics.render_frame_times;
		if frame_times.paused {
			return;
		}

		frame_times.times.push_back(frame_time);
		if frame_times.times.len() > frame_times.max_len {
			_ = frame_times.times.drain(..frame_times.times.len() - frame_times.max_len);
		}
	}

	/// Pauses the frame times for a window
	pub fn render_frame_times_pause(&self, window_id: WindowId, pause: bool) {
		self.window_metrics
			.lock()
			.entry(window_id)
			.or_default()
			.render_frame_times
			.paused = pause;
	}

	/// Returns if the frame time is paused
	pub fn render_frame_times_is_paused(&self, window_id: WindowId) -> bool {
		match self.window_metrics.lock().get(&window_id) {
			Some(metrics) => metrics.render_frame_times.paused,
			None => true,
		}
	}

	/// Returns the max number of frame times kept
	pub fn render_frame_times_max_len(&self, window_id: WindowId) -> usize {
		match self.window_metrics.lock().get(&window_id) {
			Some(metrics) => metrics.render_frame_times.max_len,
			None => 0,
		}
	}

	/// Sets the max number of frame times kept
	pub fn render_frame_times_set_max_len(&self, window_id: WindowId, max_len: usize) {
		self.window_metrics
			.lock()
			.entry(window_id)
			.or_default()
			.render_frame_times
			.max_len = max_len;
	}

	/// Returns the frame times from the metrics
	pub fn render_frame_times(&self) -> BTreeMap<WindowId, Vec<RenderFrameTime>> {
		self.window_metrics
			.lock()
			.iter()
			.map(|(&window_id, window_metrics)| {
				(
					window_id,
					window_metrics.render_frame_times.times.iter().copied().collect(),
				)
			})
			.collect()
	}

	/// Returns all window ids from the metrics
	pub fn window_ids<C>(&self) -> C
	where
		C: FromIterator<WindowId>,
	{
		self.window_metrics.lock().keys().copied().collect()
	}
}

/// Render frame times
#[derive(Debug)]
struct RenderFrameTimes {
	/// Render frame times
	times: VecDeque<RenderFrameTime>,

	/// Maximum frame times
	max_len: usize,

	/// Paused
	paused: bool,
}

impl Default for RenderFrameTimes {
	fn default() -> Self {
		Self {
			times:   VecDeque::new(),
			// 10 seconds worth on 60 Hz
			max_len: 60 * 10,
			paused:  false,
		}
	}
}

/// Render frame time.
///
/// These are the durations (cumulative by order) that it
/// took to perform each step of the frame
#[derive(Clone, Copy, Debug)]
pub struct RenderFrameTime {
	pub paint_egui:    Duration,
	pub render_start:  Duration,
	pub render_panels: Duration,
	pub render_egui:   Duration,
	pub render_finish: Duration,
	pub resize:        Duration,
}
