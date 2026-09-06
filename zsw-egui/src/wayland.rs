//! Egui wayland state

use {
	core::mem,
	euclid::default::Vector2D,
	smithay_client_toolkit::seat::{
		keyboard::{Modifiers, RawModifiers},
		pointer::{self, CursorIcon, PointerEvent, PointerEventKind, PointerThemeError},
	},
	std::time::Instant,
	xkeysym::Keysym,
	zsw_wayland::{WaylandData, WaylandEventLoop},
	zsw_wgpu::WgpuRenderer,
};

/// Egui wayland state
#[derive(Debug)]
pub struct EguiWaylandState {
	screen_rect: Option<egui::Rect>,

	current_modifiers: egui::Modifiers,
	events:            Vec<egui::Event>,

	focused:          bool,
	max_texture_side: Option<usize>,

	start:     Instant,
	last_take: Instant,
}

impl EguiWaylandState {
	/// Creates a new, empty, state
	#[expect(
		clippy::new_without_default,
		reason = "We want to be explicit about creating the state"
	)]
	#[must_use]
	pub fn new() -> Self {
		let now = Instant::now();
		Self {
			screen_rect:       None,
			current_modifiers: egui::Modifiers::default(),
			events:            vec![],
			focused:           false,
			max_texture_side:  None,
			start:             now,
			last_take:         now,
		}
	}

	/// Takes the raw input since last frame
	pub fn take_input(&mut self) -> egui::RawInput {
		let viewport_id = egui::ViewportId::default();
		let mut viewports = egui::ViewportIdMap::default();
		_ = viewports.insert(viewport_id, egui::ViewportInfo {
			parent:                  None,
			title:                   None,
			events:                  Vec::new(),
			native_pixels_per_point: None,
			monitor_size:            None,
			inner_rect:              None,
			outer_rect:              None,
			minimized:               None,
			maximized:               None,
			fullscreen:              None,
			focused:                 Some(self.focused),
			occluded:                None,
		});

		let now = Instant::now();
		let time = (self.last_take - self.start).as_secs_f64();
		let predicted_dt = (now - self.last_take).as_secs_f32();
		self.last_take = now;

		egui::RawInput {
			viewport_id,
			viewports,
			safe_area_insets: None,
			screen_rect: self.screen_rect,
			max_texture_side: self.max_texture_side,
			time: Some(time),
			predicted_dt,
			events: mem::take(&mut self.events),
			hovered_files: Vec::new(),
			dropped_files: Vec::new(),
			focused: self.focused,
			system_theme: None,
		}
	}

	/// Updates this state when wgpu is created
	pub fn update_wgpu(&mut self, wgpu: &WgpuRenderer) {
		self.max_texture_side = Some(wgpu.device.limits().max_texture_dimension_2d as usize);
	}

	/// Updates this state with the egui output
	pub fn update_output<A>(
		&mut self,
		wayland_event_loop: &mut WaylandEventLoop<A>,
		wayland_data: &mut WaylandData<A>,
		output: egui::PlatformOutput,
	) {
		let egui::PlatformOutput {
			commands,
			cursor_icon,
			cursor_image: _,
			events: _,
			mutable_text_under_cursor: _,
			ime: _,
			accesskit_update: _,
			num_completed_passes: _,
			request_discard_reasons: _,
		} = output;

		for command in commands {
			match command {
				egui::OutputCommand::CopyText(text) => wayland_data.clipboard.store(text),
				_ => tracing::warn!(?command, "Ignoring egui command"),
			}
		}

		if let Some(pointer) = &mut wayland_data.pointer {
			match self::egui_icon_to_cursor_icon(cursor_icon) {
				Some(icon) =>
					if let Err(err) = pointer.set_cursor(wayland_event_loop.conn(), icon) &&
						!matches!(err, PointerThemeError::MissingEnterSerial)
					{
						tracing::warn!(?cursor_icon, ?icon, ?err, "Unable to set pointer");
					},
				None =>
					if let Err(err) = pointer.hide_cursor() {
						tracing::warn!(?cursor_icon, ?err, "Unable to hide pointer");
					},
			}
		}
	}

	/// Updates the surface size
	pub fn update_surface_size(&mut self, surface_size: Vector2D<u32>) {
		self.screen_rect = Some(egui::Rect {
			min: egui::pos2(0.0, 0.0),
			max: egui::pos2(surface_size.x as f32, surface_size.y as f32),
		});
	}

	/// Updates a keyboard key
	pub fn update_keyboard_key<A>(
		&mut self,
		wayland_data: &WaylandData<A>,
		keysym: Keysym,
		raw: u32,
		text: Option<String>,
		state: zsw_wayland::KeyboardKeyState,
	) {
		let Some(key) = self::egui_key(keysym) else {
			tracing::warn!(?keysym, raw, text, "Ignoring unknown key event");
			return;
		};

		self.events.push(egui::Event::Key {
			key,
			physical_key: None,
			pressed: matches!(state, zsw_wayland::KeyboardKeyState::Pressed),
			repeat: matches!(state, zsw_wayland::KeyboardKeyState::Repeat),
			modifiers: self.current_modifiers,
		});

		if let Some(text) = text &&
			!text.contains(char::is_control) &&
			!text.is_empty()
		{
			self.events.push(egui::Event::Text(text));
		}

		if key == egui::Key::Cut || (self.current_modifiers.command && key == egui::Key::X) {
			self.events.push(egui::Event::Cut);
		}
		if key == egui::Key::Copy || (self.current_modifiers.command && key == egui::Key::C) {
			self.events.push(egui::Event::Copy);
		}
		if key == egui::Key::Paste || (self.current_modifiers.command && key == egui::Key::V) {
			match wayland_data.clipboard.load() {
				Ok(text) => self.events.push(egui::Event::Paste(text)),
				Err(err) => tracing::warn!(?err, "Unable to get clipboard"),
			}
		}
	}

	/// Updates the keyboard modifiers
	pub fn update_keyboard_modifiers(&mut self, modifiers: Modifiers, _raw_modifiers: RawModifiers) {
		let Modifiers {
			ctrl,
			alt,
			shift,
			caps_lock: _,
			logo: _,
			num_lock: _,
		} = modifiers;

		self.current_modifiers.ctrl = ctrl;
		self.current_modifiers.alt = alt;
		self.current_modifiers.shift = shift;
		self.current_modifiers.mac_cmd = false;
		self.current_modifiers.command = ctrl;

		self.events.push(egui::Event::ModifiersChanged(self.current_modifiers));
	}

	/// Updates whether the keyboard is focused
	pub fn update_keyboard_focus(&mut self, focused: bool) {
		self.focused = focused;
	}

	/// Updates the pointer state
	pub fn update_pointer(&mut self, events: &[PointerEvent]) {
		for event in events {
			let pos = egui::pos2(event.position.0 as f32, event.position.1 as f32);
			let event = match event.kind {
				PointerEventKind::Enter { .. } => continue,
				PointerEventKind::Leave { .. } => egui::Event::PointerGone,
				PointerEventKind::Motion { .. } => egui::Event::PointerMoved(pos),
				PointerEventKind::Press { button, .. } => egui::Event::PointerButton {
					pos,
					button: match self::egui_button(button) {
						Some(value) => value,
						None => continue,
					},
					pressed: true,
					modifiers: self.current_modifiers,
				},
				PointerEventKind::Release { button, .. } => egui::Event::PointerButton {
					pos,
					button: match self::egui_button(button) {
						Some(value) => value,
						None => continue,
					},
					pressed: false,
					modifiers: self.current_modifiers,
				},
				PointerEventKind::Axis {
					horizontal, vertical, ..
				} => egui::Event::MouseWheel {
					unit:      egui::MouseWheelUnit::Line,
					// TODO: Should we be inverting the y value here? Egui seems to expect it
					delta:     egui::vec2(horizontal.value120 as f32, -vertical.value120 as f32) / 120.0,
					phase:     egui::TouchPhase::Move,
					modifiers: self.current_modifiers,
				},
			};
			self.events.push(event);
		}
	}
}

/// Gets the egui key of a wayland event
#[expect(clippy::too_many_lines, reason = "It cannot be smaller, it's a lookup table")]
fn egui_key(key: Keysym) -> Option<egui::Key> {
	let key = match key {
		Keysym::Down => egui::Key::ArrowDown,
		Keysym::Left => egui::Key::ArrowLeft,
		Keysym::Right => egui::Key::ArrowRight,
		Keysym::Up => egui::Key::ArrowUp,
		Keysym::Escape => egui::Key::Escape,
		Keysym::Tab => egui::Key::Tab,
		Keysym::BackSpace => egui::Key::Backspace,
		Keysym::Return => egui::Key::Enter,
		Keysym::space => egui::Key::Space,
		Keysym::Insert => egui::Key::Insert,
		Keysym::Delete => egui::Key::Delete,
		Keysym::Home => egui::Key::Home,
		Keysym::End => egui::Key::End,
		Keysym::Page_Up => egui::Key::PageUp,
		Keysym::Page_Down => egui::Key::PageDown,
		Keysym::OSF_Copy | Keysym::SUN_Copy | Keysym::XF86_Copy => egui::Key::Copy,
		Keysym::OSF_Cut | Keysym::SUN_Cut | Keysym::XF86_Cut => egui::Key::Cut,
		Keysym::OSF_Paste |
		Keysym::OSF_QuickPaste |
		Keysym::OSF_PrimaryPaste |
		Keysym::SUN_Paste |
		Keysym::XF86_Paste => egui::Key::Paste,
		Keysym::colon => egui::Key::Colon,
		Keysym::comma => egui::Key::Comma,
		Keysym::backslash => egui::Key::Backslash,
		Keysym::slash => egui::Key::Slash,
		Keysym::bar => egui::Key::Pipe,
		Keysym::question => egui::Key::Questionmark,
		Keysym::exclam => egui::Key::Exclamationmark,
		Keysym::bracketleft => egui::Key::OpenBracket,
		Keysym::bracketright => egui::Key::CloseBracket,
		Keysym::braceleft => egui::Key::OpenCurlyBracket,
		Keysym::braceright => egui::Key::CloseCurlyBracket,
		Keysym::grave | Keysym::dead_grave => egui::Key::Backtick,
		Keysym::minus => egui::Key::Minus,
		Keysym::period => egui::Key::Period,
		Keysym::plus => egui::Key::Plus,
		Keysym::equal => egui::Key::Equals,
		Keysym::semicolon => egui::Key::Semicolon,
		Keysym::apostrophe => egui::Key::Quote,
		Keysym::_0 | Keysym::KP_0 => egui::Key::Num0,
		Keysym::_1 | Keysym::KP_1 => egui::Key::Num1,
		Keysym::_2 | Keysym::KP_2 => egui::Key::Num2,
		Keysym::_3 | Keysym::KP_3 => egui::Key::Num3,
		Keysym::_4 | Keysym::KP_4 => egui::Key::Num4,
		Keysym::_5 | Keysym::KP_5 => egui::Key::Num5,
		Keysym::_6 | Keysym::KP_6 => egui::Key::Num6,
		Keysym::_7 | Keysym::KP_7 => egui::Key::Num7,
		Keysym::_8 | Keysym::KP_8 => egui::Key::Num8,
		Keysym::_9 | Keysym::KP_9 => egui::Key::Num9,
		Keysym::a | Keysym::A => egui::Key::A,
		Keysym::b | Keysym::B => egui::Key::B,
		Keysym::c | Keysym::C => egui::Key::C,
		Keysym::d | Keysym::D => egui::Key::D,
		Keysym::e | Keysym::E => egui::Key::E,
		Keysym::f | Keysym::F => egui::Key::F,
		Keysym::g | Keysym::G => egui::Key::G,
		Keysym::h | Keysym::H => egui::Key::H,
		Keysym::i | Keysym::I => egui::Key::I,
		Keysym::j | Keysym::J => egui::Key::J,
		Keysym::k | Keysym::K => egui::Key::K,
		Keysym::l | Keysym::L => egui::Key::L,
		Keysym::m | Keysym::M => egui::Key::M,
		Keysym::n | Keysym::N => egui::Key::N,
		Keysym::o | Keysym::O => egui::Key::O,
		Keysym::p | Keysym::P => egui::Key::P,
		Keysym::q | Keysym::Q => egui::Key::Q,
		Keysym::r | Keysym::R => egui::Key::R,
		Keysym::s | Keysym::S => egui::Key::S,
		Keysym::t | Keysym::T => egui::Key::T,
		Keysym::u | Keysym::U => egui::Key::U,
		Keysym::v | Keysym::V => egui::Key::V,
		Keysym::w | Keysym::W => egui::Key::W,
		Keysym::x | Keysym::X => egui::Key::X,
		Keysym::y | Keysym::Y => egui::Key::Y,
		Keysym::z | Keysym::Z => egui::Key::Z,
		Keysym::F1 => egui::Key::F1,
		Keysym::F2 => egui::Key::F2,
		Keysym::F3 => egui::Key::F3,
		Keysym::F4 => egui::Key::F4,
		Keysym::F5 => egui::Key::F5,
		Keysym::F6 => egui::Key::F6,
		Keysym::F7 => egui::Key::F7,
		Keysym::F8 => egui::Key::F8,
		Keysym::F9 => egui::Key::F9,
		Keysym::F10 => egui::Key::F10,
		Keysym::F11 => egui::Key::F11,
		Keysym::F12 => egui::Key::F12,
		Keysym::F13 => egui::Key::F13,
		Keysym::F14 => egui::Key::F14,
		Keysym::F15 => egui::Key::F15,
		Keysym::F16 => egui::Key::F16,
		Keysym::F17 => egui::Key::F17,
		Keysym::F18 => egui::Key::F18,
		Keysym::F19 => egui::Key::F19,
		Keysym::F20 => egui::Key::F20,
		Keysym::F21 => egui::Key::F21,
		Keysym::F22 => egui::Key::F22,
		Keysym::F23 => egui::Key::F23,
		Keysym::F24 => egui::Key::F24,
		Keysym::F25 => egui::Key::F25,
		Keysym::F26 => egui::Key::F26,
		Keysym::F27 => egui::Key::F27,
		Keysym::F28 => egui::Key::F28,
		Keysym::F29 => egui::Key::F29,
		Keysym::F30 => egui::Key::F30,
		Keysym::F31 => egui::Key::F31,
		Keysym::F32 => egui::Key::F32,
		Keysym::F33 => egui::Key::F33,
		Keysym::F34 => egui::Key::F34,
		Keysym::F35 => egui::Key::F35,
		Keysym::XF86_Back => egui::Key::BrowserBack,
		Keysym::Shift_L => egui::Key::ShiftLeft,
		Keysym::Shift_R => egui::Key::ShiftRight,
		Keysym::Control_L => egui::Key::ControlLeft,
		Keysym::Control_R => egui::Key::ControlRight,
		Keysym::Alt_L => egui::Key::AltLeft,
		Keysym::Alt_R => egui::Key::AltRight,
		Keysym::Super_L => egui::Key::SuperLeft,
		Keysym::Super_R => egui::Key::SuperRight,
		_ => return None,
	};

	Some(key)
}

/// Gets the egui button of a wayland event
fn egui_button(button: u32) -> Option<egui::PointerButton> {
	let button = match button {
		pointer::BTN_LEFT => egui::PointerButton::Primary,
		pointer::BTN_RIGHT => egui::PointerButton::Secondary,
		pointer::BTN_MIDDLE => egui::PointerButton::Middle,
		pointer::BTN_SIDE | pointer::BTN_FORWARD => egui::PointerButton::Extra1,
		pointer::BTN_EXTRA | pointer::BTN_BACK => egui::PointerButton::Extra2,
		_ => return None,
	};
	Some(button)
}

/// Converts an egui icon to a cursor icon.
///
/// `None` means the cursor should be hidden, not that it's an unknown cursor icon
fn egui_icon_to_cursor_icon(cursor_icon: egui::CursorIcon) -> Option<CursorIcon> {
	let icon = match cursor_icon {
		egui::CursorIcon::Default => CursorIcon::Default,
		egui::CursorIcon::None => return None,
		egui::CursorIcon::ContextMenu => CursorIcon::ContextMenu,
		egui::CursorIcon::Help => CursorIcon::Help,
		egui::CursorIcon::PointingHand => CursorIcon::Pointer,
		egui::CursorIcon::Progress => CursorIcon::Progress,
		egui::CursorIcon::Wait => CursorIcon::Wait,
		egui::CursorIcon::Cell => CursorIcon::Cell,
		egui::CursorIcon::Crosshair => CursorIcon::Crosshair,
		egui::CursorIcon::Text => CursorIcon::Text,
		egui::CursorIcon::VerticalText => CursorIcon::VerticalText,
		egui::CursorIcon::Alias => CursorIcon::Alias,
		egui::CursorIcon::Copy => CursorIcon::Copy,
		egui::CursorIcon::Move => CursorIcon::Move,
		egui::CursorIcon::NoDrop => CursorIcon::NoDrop,
		egui::CursorIcon::NotAllowed => CursorIcon::NotAllowed,
		egui::CursorIcon::Grab => CursorIcon::Grab,
		egui::CursorIcon::Grabbing => CursorIcon::Grabbing,
		egui::CursorIcon::AllScroll => CursorIcon::AllScroll,
		egui::CursorIcon::ResizeHorizontal => CursorIcon::EwResize,
		egui::CursorIcon::ResizeNeSw => CursorIcon::NeswResize,
		egui::CursorIcon::ResizeNwSe => CursorIcon::NwseResize,
		egui::CursorIcon::ResizeVertical => CursorIcon::NsResize,
		egui::CursorIcon::ResizeEast => CursorIcon::EResize,
		egui::CursorIcon::ResizeSouthEast => CursorIcon::SeResize,
		egui::CursorIcon::ResizeSouth => CursorIcon::SResize,
		egui::CursorIcon::ResizeSouthWest => CursorIcon::SwResize,
		egui::CursorIcon::ResizeWest => CursorIcon::WResize,
		egui::CursorIcon::ResizeNorthWest => CursorIcon::NwResize,
		egui::CursorIcon::ResizeNorth => CursorIcon::NResize,
		egui::CursorIcon::ResizeNorthEast => CursorIcon::NeResize,
		egui::CursorIcon::ResizeColumn => CursorIcon::ColResize,
		egui::CursorIcon::ResizeRow => CursorIcon::RowResize,
		egui::CursorIcon::ZoomIn => CursorIcon::ZoomIn,
		egui::CursorIcon::ZoomOut => CursorIcon::ZoomOut,
	};

	Some(icon)
}
