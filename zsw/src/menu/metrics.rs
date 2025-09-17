//! Metrics tab

// Modules
mod frame_times;

// Imports
use {
	crate::{metrics::Metrics, shared::SharedWindow},
	std::{
		collections::{BTreeSet, HashMap},
		sync::Arc,
	},
	winit::window::WindowId,
	zsw_util::TokioTaskBlockOn,
};

/// Draws the metrics tab
pub fn draw_metrics_tab(ui: &mut egui::Ui, metrics: &Metrics, shared_windows: &HashMap<WindowId, Arc<SharedWindow>>) {
	let cur_metric = super::get_data::<Metric>(ui, "metrics-tab-cur-metric");
	let mut cur_metric = cur_metric.lock();
	ui.horizontal(|ui| {
		let metrics = [
			Metric::Window(WindowMetric::Render),
			Metric::Window(WindowMetric::RenderPanels),
		];

		ui.label("Metric: ");
		for metric in metrics {
			ui.selectable_value(&mut *cur_metric, metric, metric.to_string());
		}
	});

	match *cur_metric {
		Metric::Window(metric) => {
			// Get the window, otherwise we have nothing to render
			let Some(window_id) = self::draw_window_select(ui, metrics, shared_windows) else {
				ui.weak("No window selected");
				return;
			};

			match metric {
				WindowMetric::Render =>
					frame_times::render::draw(ui, &mut metrics.render_frame_times(window_id).block_on()),
				WindowMetric::RenderPanels =>
					frame_times::render_panels::draw(ui, &mut metrics.render_panels_frame_times(window_id).block_on()),
			}
		},
	}
}

/// Draws the window select and returns the selected one
fn draw_window_select(
	ui: &mut egui::Ui,
	metrics: &Metrics,
	shared_windows: &HashMap<WindowId, Arc<SharedWindow>>,
) -> Option<WindowId> {
	let cur_window_id = super::get_data::<Option<WindowId>>(ui, "metrics-tab-cur-window");
	let mut cur_window_id = cur_window_id.lock();

	let window_name = |window_id| {
		shared_windows.get(&window_id).map_or_else(
			|| format!("{window_id:?}"),
			|shared_window| shared_window.monitor_name.clone(),
		)
	};

	let windows = metrics.window_ids::<BTreeSet<_>>().block_on();

	// If we don't have a current window, use the first one available (if any)
	if cur_window_id.is_none() &&
		let Some(&window_id) = windows.first()
	{
		*cur_window_id = Some(window_id);
	}

	ui.horizontal(|ui| {
		ui.label("Window: ");
		egui::ComboBox::from_id_salt("metrics-tab-window-selector")
			.selected_text(cur_window_id.map_or("None".to_owned(), window_name))
			.show_ui(ui, |ui| {
				for window_id in windows {
					ui.selectable_value(&mut *cur_window_id, Some(window_id), window_name(window_id));
				}
			});
	});

	*cur_window_id
}


/// Metric to show
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[derive(derive_more::Display)]
enum Metric {
	Window(WindowMetric),
}

impl Default for Metric {
	fn default() -> Self {
		Self::Window(WindowMetric::default())
	}
}

/// Window metric to show
#[derive(PartialEq, Eq, Clone, Copy, Default, Debug)]
#[derive(derive_more::Display)]
#[derive(strum::EnumIter)]
enum WindowMetric {
	#[default]
	#[display("Render")]
	Render,

	#[display("Render panels")]
	RenderPanels,
}
