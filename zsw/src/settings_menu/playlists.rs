//! Playlists tab

// Imports
use {crate::playlist::Playlists, std::sync::Arc, zsw_util::TokioTaskBlockOn, zutil_cloned::cloned};

/// Draws the playlists tab
pub fn draw_playlists_tab(ui: &mut egui::Ui, playlists: &Arc<Playlists>) {
	for playlist in playlists.get_all().block_on() {
		let playlist = playlist.lock().block_on();

		ui.collapsing(playlist.name.to_string(), |ui| {
			if ui.button("Save").clicked() {
				#[cloned(playlists, playlist_name = playlist.name;)]
				crate::spawn_task(format!("Save playlist {playlist_name:?}"), async move {
					playlists.save(&playlist_name).await
				});
			}
		});
	}
}
