//! Frame times metrics

// Modules
pub mod render;

// Imports
use {
	crate::{metrics::FrameTimes, settings_menu},
	core::time::Duration,
	egui::Widget,
	std::collections::HashMap,
};

struct FrameTimeSettings {
	is_histogram:         bool,
	histogram_time_scale: f64,
	stack_charts:         bool,
}

/// Draws a frame time's settings
fn draw_frame_time_settings<T>(ui: &mut egui::Ui, frame_times: &mut FrameTimes<T>) -> FrameTimeSettings {
	// TODO: Turn this into some enum between histogram / time
	let is_histogram = settings_menu::get_data::<bool>(ui, "metrics-tab-histogram");
	let mut is_histogram = is_histogram.lock();

	let histogram_time_scale =
		settings_menu::get_data_with_default::<f64>(ui, "metrics-tab-histogram-time-scale", || 20.0);
	let mut histogram_time_scale = histogram_time_scale.lock();

	let stack_charts = settings_menu::get_data_with_default::<bool>(ui, "metrics-tab-chart-stacks", || true);
	let mut stack_charts = stack_charts.lock();

	ui.horizontal(|ui| {
		let mut is_paused = frame_times.is_paused();
		if ui.toggle_value(&mut is_paused, "Pause").changed() {
			frame_times.pause(is_paused);
		}

		let mut max_len = frame_times.max_len();
		ui.horizontal(|ui| {
			ui.label("Maximum frames: ");
			if egui::Slider::new(&mut max_len, 0..=60 * 100).ui(ui).changed() {
				frame_times.set_max_len(max_len);
			}
		});

		ui.toggle_value(&mut is_histogram, "Histogram");

		match *is_histogram {
			true => {
				ui.horizontal(|ui| {
					ui.label("Time scale: ");
					egui::Slider::new(&mut *histogram_time_scale, 1.0..=1000.0)
						.logarithmic(true)
						.clamping(egui::SliderClamping::Always)
						.ui(ui);
				});
			},
			false => {
				ui.toggle_value(&mut stack_charts, "Stack charts");
			},
		}
	});

	FrameTimeSettings {
		is_histogram:         *is_histogram,
		histogram_time_scale: *histogram_time_scale,
		stack_charts:         *stack_charts,
	}
}

/// Creates a chart of frame times
fn add_frame_time_chart<T, D>(
	frame_times: &FrameTimes<T>,
	is_histogram: bool,
	histogram_time_scale: f64,
	stack_charts: bool,
	prev_charts: &[egui_plot::BarChart],
	duration_idx: &D,
) -> egui_plot::BarChart
where
	D: DurationIdx<T>,
{
	let bars = match is_histogram {
		true => {
			let mut buckets: HashMap<usize, usize> = HashMap::<_, usize>::new();
			for frame_time in frame_times.iter() {
				let Some(duration) = duration_idx.duration_of(frame_time) else {
					continue;
				};
				#[expect(clippy::cast_sign_loss, reason = "Durations are positive")]
				let bucket_idx = (duration.as_millis_f64() * histogram_time_scale) as usize;

				*buckets.entry(bucket_idx).or_default() += 1;
			}

			buckets
				.into_iter()
				.map(|(bucket_idx, bucket)| {
					let width = 1.0 / histogram_time_scale;
					let center = bucket_idx as f64 / histogram_time_scale + width / 2.0;
					let height = histogram_time_scale * bucket as f64 / frame_times.len() as f64;

					egui_plot::Bar::new(center, height).width(width)
				})
				.collect()
		},
		false => frame_times
			.iter()
			.enumerate()
			.filter_map(|(frame_idx, frame_time)| {
				let height = duration_idx.duration_of(frame_time)?.as_millis_f64();
				Some(egui_plot::Bar::new(frame_idx as f64, height).width(1.0))
			})
			.collect(),
	};

	let mut chart = egui_plot::BarChart::new(duration_idx.name(), bars);
	if !is_histogram && stack_charts {
		chart = chart.stack_on(&prev_charts.iter().collect::<Vec<_>>());
	}
	chart
}

/// Duration index
pub trait DurationIdx<T> {
	/// Returns the name of this index
	fn name(&self) -> String;

	/// Returns the duration relative to this index
	fn duration_of(&self, frame_time: &T) -> Option<Duration>;
}
