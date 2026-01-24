use slint::LogicalPosition;
use slint::platform::WindowEvent;
use wayland_client::WEnum;
use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::protocol::{wl_keyboard, wl_pointer};

use super::state::ActiveLockSurface;
use crate::wayland::input::keyboard::{
    handle_keyboard_enter as shared_keyboard_enter, handle_keyboard_key as shared_keyboard_key,
    handle_keyboard_leave as shared_keyboard_leave,
};
use crate::wayland::input::pointer::{
    handle_axis as shared_axis, handle_axis_discrete as shared_axis_discrete,
    handle_axis_source as shared_axis_source, handle_axis_stop as shared_axis_stop,
    handle_pointer_button as shared_pointer_button, handle_pointer_enter as shared_pointer_enter,
    handle_pointer_frame as shared_pointer_frame, handle_pointer_leave as shared_pointer_leave,
    handle_pointer_motion as shared_pointer_motion,
};
use crate::wayland::input::{
    KeyboardEventTarget, KeyboardInputState, KeyboardSurfaceResolver, PointerEventTarget,
    PointerInputState, PointerSurfaceResolver,
};
use crate::wayland::surfaces::keyboard_state::KeyboardState;

pub(super) struct InputState {
    pub pointer: PointerInputState,
    pub keyboard: KeyboardInputState,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            pointer: PointerInputState::new(),
            keyboard: KeyboardInputState::new(),
        }
    }

    pub fn reset(&mut self) {
        self.pointer.reset();
        self.keyboard.reset();
    }

    pub fn clear_surface_refs(&mut self, surface_id: &ObjectId) {
        self.pointer.clear_surface_if_matches(surface_id);
        self.keyboard.clear_surface_if_matches(surface_id);
    }

    pub const fn has_active_pointer(&self) -> bool {
        self.pointer.has_active_surface()
    }

    pub const fn has_keyboard_focus(&self) -> bool {
        self.keyboard.has_focused_surface()
    }
}

impl PointerEventTarget for ActiveLockSurface {
    fn to_logical_position(&self, surface_x: f64, surface_y: f64) -> LogicalPosition {
        ActiveLockSurface::to_logical_position(self, surface_x, surface_y)
    }

    fn dispatch_event(&self, event: WindowEvent) {
        ActiveLockSurface::dispatch_event(self, event);
    }
}

impl KeyboardEventTarget for ActiveLockSurface {
    fn dispatch_event(&self, event: WindowEvent) {
        ActiveLockSurface::dispatch_event(self, event);
    }
}

struct LockSurfaceResolver<'a> {
    lock_surfaces: &'a [(ObjectId, ActiveLockSurface)],
}

impl PointerSurfaceResolver for LockSurfaceResolver<'_> {
    type Target = ActiveLockSurface;

    fn find_surface(&self, surface_id: &ObjectId) -> Option<&Self::Target> {
        find_surface_by_surface_id(self.lock_surfaces, surface_id)
    }
}

impl KeyboardSurfaceResolver for LockSurfaceResolver<'_> {
    type Target = ActiveLockSurface;

    fn find_surface(&self, surface_id: &ObjectId) -> Option<&Self::Target> {
        find_surface_by_surface_id(self.lock_surfaces, surface_id)
    }
}

pub(super) fn handle_pointer_enter(
    input_state: &mut InputState,
    lock_surfaces: &[(ObjectId, ActiveLockSurface)],
    serial: u32,
    surface: &WlSurface,
    surface_x: f64,
    surface_y: f64,
) -> bool {
    let resolver = LockSurfaceResolver { lock_surfaces };
    shared_pointer_enter(
        &mut input_state.pointer,
        &resolver,
        serial,
        surface,
        surface_x,
        surface_y,
    )
}

pub(super) fn handle_pointer_motion(
    input_state: &mut InputState,
    lock_surfaces: &[(ObjectId, ActiveLockSurface)],
    surface_x: f64,
    surface_y: f64,
) -> bool {
    let resolver = LockSurfaceResolver { lock_surfaces };
    shared_pointer_motion(&mut input_state.pointer, &resolver, surface_x, surface_y)
}

pub(super) fn handle_pointer_leave(
    input_state: &mut InputState,
    lock_surfaces: &[(ObjectId, ActiveLockSurface)],
) -> bool {
    let resolver = LockSurfaceResolver { lock_surfaces };
    shared_pointer_leave(&mut input_state.pointer, &resolver)
}

pub(super) fn handle_pointer_button(
    input_state: &mut InputState,
    lock_surfaces: &[(ObjectId, ActiveLockSurface)],
    scale_factor: f32,
    serial: u32,
    button: u32,
    button_state: WEnum<wl_pointer::ButtonState>,
) -> bool {
    let resolver = LockSurfaceResolver { lock_surfaces };
    shared_pointer_button(
        &input_state.pointer,
        &resolver,
        scale_factor,
        serial,
        button,
        button_state,
    )
}

pub(super) fn handle_axis_source(
    input_state: &InputState,
    axis_source: wl_pointer::AxisSource,
) -> bool {
    shared_axis_source(&input_state.pointer, axis_source)
}

pub(super) fn handle_axis(
    input_state: &mut InputState,
    axis: wl_pointer::Axis,
    value: f64,
) -> bool {
    shared_axis(&mut input_state.pointer, axis, value)
}

pub(super) fn handle_axis_discrete(
    input_state: &mut InputState,
    axis: wl_pointer::Axis,
    discrete: i32,
) -> bool {
    shared_axis_discrete(&mut input_state.pointer, axis, discrete)
}

pub(super) fn handle_axis_stop(input_state: &InputState, axis: wl_pointer::Axis) -> bool {
    shared_axis_stop(&input_state.pointer, axis)
}

pub(super) fn handle_pointer_frame(
    input_state: &mut InputState,
    lock_surfaces: &[(ObjectId, ActiveLockSurface)],
) -> bool {
    let resolver = LockSurfaceResolver { lock_surfaces };
    shared_pointer_frame(&mut input_state.pointer, &resolver)
}

pub(super) fn handle_keyboard_enter(
    input_state: &mut InputState,
    lock_surfaces: &[(ObjectId, ActiveLockSurface)],
    surface: &WlSurface,
) -> bool {
    let resolver = LockSurfaceResolver { lock_surfaces };
    shared_keyboard_enter(&mut input_state.keyboard, &resolver, surface)
}

pub(super) fn handle_keyboard_leave(input_state: &mut InputState, surface: &WlSurface) -> bool {
    shared_keyboard_leave(&mut input_state.keyboard, surface)
}

pub(super) fn handle_keyboard_key(
    input_state: &InputState,
    lock_surfaces: &[(ObjectId, ActiveLockSurface)],
    key: u32,
    state: wl_keyboard::KeyState,
    keyboard_state: &mut KeyboardState,
) -> bool {
    let resolver = LockSurfaceResolver { lock_surfaces };
    shared_keyboard_key(&input_state.keyboard, &resolver, key, state, keyboard_state)
}

fn find_surface_by_surface_id<'a>(
    lock_surfaces: &'a [(ObjectId, ActiveLockSurface)],
    surface_id: &ObjectId,
) -> Option<&'a ActiveLockSurface> {
    lock_surfaces
        .iter()
        .find(|(_, surface)| surface.surface().surface_id() == *surface_id)
        .map(|(_, surface)| surface)
}
