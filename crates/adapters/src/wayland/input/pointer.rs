use slint::LogicalPosition;
use slint::platform::WindowEvent;
use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_pointer;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Proxy, WEnum};

use super::state::PointerInputState;
use crate::wayland::surfaces::pointer_utils::wayland_button_to_slint;

pub trait PointerEventTarget {
    fn to_logical_position(&self, surface_x: f64, surface_y: f64) -> LogicalPosition;
    fn dispatch_event(&self, event: WindowEvent);
}

pub trait PointerSurfaceResolver {
    type Target: PointerEventTarget;

    fn find_surface(&self, surface_id: &ObjectId) -> Option<&Self::Target>;
}

pub fn handle_pointer_enter<R: PointerSurfaceResolver>(
    input_state: &mut PointerInputState,
    resolver: &R,
    _serial: u32,
    surface: &WlSurface,
    surface_x: f64,
    surface_y: f64,
) -> bool {
    let surface_id = surface.id();
    let Some(target) = resolver.find_surface(&surface_id) else {
        return false;
    };

    let position = target.to_logical_position(surface_x, surface_y);
    input_state.set_active_surface(Some(surface_id));
    input_state.set_current_position(position);

    target.dispatch_event(WindowEvent::PointerMoved { position });
    true
}

pub fn handle_pointer_motion<R: PointerSurfaceResolver>(
    input_state: &mut PointerInputState,
    resolver: &R,
    surface_x: f64,
    surface_y: f64,
) -> bool {
    let Some(surface_id) = input_state.active_surface_id().cloned() else {
        return false;
    };
    let Some(target) = resolver.find_surface(&surface_id) else {
        return false;
    };

    let position = target.to_logical_position(surface_x, surface_y);
    input_state.set_current_position(position);

    target.dispatch_event(WindowEvent::PointerMoved { position });
    true
}

pub fn handle_pointer_leave<R: PointerSurfaceResolver>(
    input_state: &mut PointerInputState,
    resolver: &R,
) -> bool {
    let Some(surface_id) = input_state.active_surface_id().cloned() else {
        return false;
    };

    if let Some(target) = resolver.find_surface(&surface_id) {
        target.dispatch_event(WindowEvent::PointerExited);
    }

    input_state.set_active_surface(None);
    true
}

pub fn handle_pointer_button<R: PointerSurfaceResolver>(
    input_state: &PointerInputState,
    resolver: &R,
    _scale_factor: f32,
    _serial: u32,
    button: u32,
    button_state: WEnum<wl_pointer::ButtonState>,
) -> bool {
    let Some(surface_id) = input_state.active_surface_id() else {
        return false;
    };
    let Some(target) = resolver.find_surface(surface_id) else {
        return false;
    };

    let position = input_state.current_position();
    let slint_button = wayland_button_to_slint(button);

    let event = match button_state {
        WEnum::Value(wl_pointer::ButtonState::Pressed) => WindowEvent::PointerPressed {
            button: slint_button,
            position,
        },
        WEnum::Value(wl_pointer::ButtonState::Released) => WindowEvent::PointerReleased {
            button: slint_button,
            position,
        },
        _ => return true,
    };

    target.dispatch_event(event);
    true
}

pub fn handle_axis_source(
    input_state: &PointerInputState,
    _axis_source: wl_pointer::AxisSource,
) -> bool {
    input_state.has_active_surface()
}

pub fn handle_axis(
    input_state: &mut PointerInputState,
    axis: wl_pointer::Axis,
    value: f64,
) -> bool {
    if !input_state.has_active_surface() {
        return false;
    }

    #[allow(clippy::cast_possible_truncation)]
    let delta = value as f32;

    match axis {
        wl_pointer::Axis::HorizontalScroll => {
            input_state.accumulate_axis_value(delta, 0.0);
        }
        wl_pointer::Axis::VerticalScroll => {
            input_state.accumulate_axis_value(0.0, delta);
        }
        _ => {}
    }
    true
}

pub fn handle_axis_discrete(
    input_state: &mut PointerInputState,
    axis: wl_pointer::Axis,
    discrete: i32,
) -> bool {
    if !input_state.has_active_surface() {
        return false;
    }

    #[allow(clippy::cast_precision_loss)]
    let delta = (discrete as f32) * 60.0;

    match axis {
        wl_pointer::Axis::HorizontalScroll => {
            input_state.accumulate_axis_value(delta, 0.0);
        }
        wl_pointer::Axis::VerticalScroll => {
            input_state.accumulate_axis_value(0.0, delta);
        }
        _ => {}
    }
    true
}

pub fn handle_axis_stop(_input_state: &PointerInputState, _axis: wl_pointer::Axis) -> bool {
    true
}

pub fn handle_pointer_frame<R: PointerSurfaceResolver>(
    input_state: &mut PointerInputState,
    resolver: &R,
) -> bool {
    let Some(surface_id) = input_state.active_surface_id().cloned() else {
        return false;
    };

    let (delta_x, delta_y) = input_state.take_accumulated_axis();

    let Some(target) = resolver.find_surface(&surface_id) else {
        return false;
    };

    if delta_x.abs() > f32::EPSILON || delta_y.abs() > f32::EPSILON {
        let position = input_state.current_position();
        target.dispatch_event(WindowEvent::PointerScrolled {
            position,
            delta_x,
            delta_y,
        });
    }

    true
}
