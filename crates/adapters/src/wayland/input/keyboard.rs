use slint::SharedString;
use slint::platform::WindowEvent;
use wayland_client::Proxy;
use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_keyboard;
use wayland_client::protocol::wl_surface::WlSurface;
use xkbcommon::xkb;

use super::state::KeyboardInputState;
use crate::wayland::surfaces::keyboard_state::{KeyboardState, keysym_to_text};

pub trait KeyboardEventTarget {
    fn dispatch_event(&self, event: WindowEvent);
}

pub trait KeyboardSurfaceResolver {
    type Target: KeyboardEventTarget;

    fn find_surface(&self, surface_id: &ObjectId) -> Option<&Self::Target>;
}

pub fn handle_keyboard_enter<R: KeyboardSurfaceResolver>(
    input_state: &mut KeyboardInputState,
    resolver: &R,
    surface: &WlSurface,
) -> bool {
    let surface_id = surface.id();
    if resolver.find_surface(&surface_id).is_some() {
        input_state.set_focused_surface(Some(surface_id));
        return true;
    }
    false
}

pub fn handle_keyboard_leave(input_state: &mut KeyboardInputState, surface: &WlSurface) -> bool {
    let surface_id = surface.id();
    if input_state.focused_surface_id() == Some(&surface_id) {
        input_state.set_focused_surface(None);
        return true;
    }
    false
}

pub fn handle_keyboard_key<R: KeyboardSurfaceResolver>(
    input_state: &KeyboardInputState,
    resolver: &R,
    key: u32,
    state: wl_keyboard::KeyState,
    keyboard_state: &mut KeyboardState,
) -> bool {
    let Some(surface_id) = input_state.focused_surface_id().cloned() else {
        return false;
    };
    let Some(target) = resolver.find_surface(&surface_id) else {
        return false;
    };
    let Some(xkb_state) = keyboard_state.xkb_state.as_mut() else {
        return true;
    };

    let keycode = xkb::Keycode::new(key + 8);
    let direction = match state {
        wl_keyboard::KeyState::Pressed => xkb::KeyDirection::Down,
        wl_keyboard::KeyState::Released => xkb::KeyDirection::Up,
        _ => return true,
    };

    xkb_state.update_key(keycode, direction);

    let text = xkb_state.key_get_utf8(keycode);
    let text = if text.is_empty() {
        let keysym = xkb_state.key_get_one_sym(keycode);
        keysym_to_text(keysym)
    } else {
        Some(SharedString::from(text.as_str()))
    };

    let Some(text) = text else {
        return true;
    };

    let event = match state {
        wl_keyboard::KeyState::Pressed => WindowEvent::KeyPressed { text },
        wl_keyboard::KeyState::Released => WindowEvent::KeyReleased { text },
        _ => return true,
    };

    target.dispatch_event(event);
    true
}
