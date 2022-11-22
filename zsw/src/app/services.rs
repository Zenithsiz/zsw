//! Services

// Imports
use {
	std::sync::Arc,
	winit::window::Window,
	zsw_egui::Egui,
	zsw_img::ImageReceiver,
	zsw_input::Input,
	zsw_panels::Panels,
	zsw_playlist::PlaylistManager,
	zsw_profiles::ProfilesManager,
	zsw_renderer::Renderer,
	zsw_settings_window::SettingsWindow,
	zsw_util::ServicesBundle,
	zsw_wgpu::Wgpu,
};


/// All services
// TODO: Make a macro for service runners to not have to bound everything and then get all the services they need
#[derive(Debug)]
pub struct Services {
	/// Window
	// TODO: Not make an arc
	pub window: Arc<Window>,

	/// Wgpu
	pub wgpu: Wgpu,

	/// Image receiver
	pub image_receiver: ImageReceiver,

	/// Playlist manager
	pub playlist_manager: PlaylistManager,

	/// Profiles manager
	pub profiles_manager: ProfilesManager,

	/// Panels
	pub panels: Panels,

	/// Egui
	pub egui: Egui,

	/// Renderer
	pub renderer: Renderer,

	/// Settings window
	pub settings_window: SettingsWindow,

	/// Input
	pub input: Input,
}

impl ServicesBundle for Services {}

#[duplicate::duplicate_item(
	ty                 field;
	[ Window           ] [ window ];
	[ Wgpu             ] [ wgpu ];
	[ ImageReceiver    ] [ image_receiver ];
	[ PlaylistManager  ] [ playlist_manager ];
	[ ProfilesManager  ] [ profiles_manager ];
	[ Panels           ] [ panels ];
	[ Egui             ] [ egui ];
	[ Renderer         ] [ renderer ];
	[ SettingsWindow   ] [ settings_window ];
	[ Input            ] [ input ];
)]
impl zsw_util::Services<ty> for Services {
	fn get(&self) -> &ty {
		&self.field
	}
}
