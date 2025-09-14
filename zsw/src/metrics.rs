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

/// Inner
#[derive(Debug)]
struct Inner {
	frame_times: HashMap<WindowId, FrameTimes>,
}

/// Metrics
#[derive(Debug)]
pub struct Metrics {
	inner: Mutex<Inner>,
}

impl Metrics {
	/// Creates new, empty, metrics
	pub fn new() -> Self {
		Self {
			inner: Mutex::new(Inner {
				frame_times: HashMap::new(),
			}),
		}
	}

	/// Adds a frame time to the metrics
	pub fn frame_times_add(&self, window_id: WindowId, frame_time: FrameTime) {
		// TODO: Make this customizable?
		const MAX_FRAME_TIMES: usize = 60 * 10;

		let mut inner = self.inner.lock();
		let frame_times = inner.frame_times.entry(window_id).or_default();
		if frame_times.paused {
			return;
		}

		frame_times.times.push_back(frame_time);
		if frame_times.times.len() > MAX_FRAME_TIMES {
			_ = frame_times.times.drain(..frame_times.times.len() - MAX_FRAME_TIMES);
		}
	}

	/// Pauses the frame times for a window
	pub fn frame_times_pause(&self, window_id: WindowId, pause: bool) {
		self.inner.lock().frame_times.entry(window_id).or_default().paused = pause;
	}

	/// Returns if the frame time is paused
	pub fn frame_times_is_paused(&self, window_id: WindowId) -> bool {
		match self.inner.lock().frame_times.get(&window_id) {
			Some(frame_times) => frame_times.paused,
			None => true,
		}
	}

	/// Returns the frame times from the metrics
	pub fn frame_times(&self) -> BTreeMap<WindowId, Vec<FrameTime>> {
		self.inner
			.lock()
			.frame_times
			.iter()
			.map(|(&window_id, frame_times)| (window_id, frame_times.times.iter().copied().collect()))
			.collect()
	}
}

/// Frame times
#[derive(Default, Debug)]
struct FrameTimes {
	/// Frame times
	times: VecDeque<FrameTime>,

	/// Paused
	paused: bool,
}

/// Frame time.
///
/// These are the durations (cumulative by order) that it
/// took to perform each step of the frame
#[derive(Clone, Copy, Debug)]
pub struct FrameTime {
	pub paint_egui:    Duration,
	pub render_start:  Duration,
	pub render_panels: Duration,
	pub render_egui:   Duration,
	pub render_finish: Duration,
	pub resize:        Duration,
}
