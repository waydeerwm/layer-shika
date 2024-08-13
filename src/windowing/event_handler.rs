use crate::impl_empty_dispatch;
use log::info;
use slint::platform::{PointerEventButton, WindowEvent};
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::ZwlrLayerShellV1,
    zwlr_layer_surface_v1::{self, ZwlrLayerSurfaceV1},
};
use std::rc::Rc;
use std::{cell::RefCell, rc::Weak};
use wayland_client::{
    globals::GlobalListContents,
    protocol::{
        wl_compositor::WlCompositor,
        wl_output::{self, WlOutput},
        wl_pointer::{self, WlPointer},
        wl_registry::WlRegistry,
        wl_seat::WlSeat,
        wl_surface::WlSurface,
    },
    Connection, Dispatch, QueueHandle,
};

use super::state::WindowState;

#[derive(Clone)]
pub struct WindowEventHandler {
    state: Weak<RefCell<WindowState>>,
}

impl WindowEventHandler {
    pub fn new(state: Weak<RefCell<WindowState>>) -> Self {
        Self { state }
    }

    pub fn state(&self) -> Rc<RefCell<WindowState>> {
        self.state.upgrade().unwrap()
    }

    fn handle_pointer_enter(&mut self, surface_x: f64, surface_y: f64) {
        if let Some(state) = self.state.upgrade() {
            state
                .borrow()
                .set_current_pointer_position(surface_x, surface_y);
            if let Some(window) = state.borrow().window() {
                let logical_position = state.borrow().current_pointer_position();
                window.dispatch_event(WindowEvent::PointerMoved {
                    position: logical_position,
                });
            }
        }
    }

    fn handle_pointer_leave(&mut self) {
        if let Some(state) = self.state.upgrade() {
            if let Some(window) = state.borrow().window() {
                window.dispatch_event(WindowEvent::PointerExited);
            }
        }
    }

    fn handle_pointer_motion(&mut self, surface_x: f64, surface_y: f64) {
        if let Some(state) = self.state.upgrade() {
            state
                .borrow()
                .set_current_pointer_position(surface_x, surface_y);
            if let Some(window) = state.borrow().window() {
                let logical_position = state.borrow().current_pointer_position();
                window.dispatch_event(WindowEvent::PointerMoved {
                    position: logical_position,
                });
            }
        }
    }

    fn handle_pointer_button(
        &mut self,
        button_state: wayland_client::WEnum<wl_pointer::ButtonState>,
    ) {
        if let Some(state) = self.state.upgrade() {
            let is_press = matches!(
                button_state,
                wayland_client::WEnum::Value(wl_pointer::ButtonState::Pressed)
            );
            let current_position = state.borrow().current_pointer_position();
            if let Some(window) = state.borrow().window() {
                let event = if is_press {
                    WindowEvent::PointerPressed {
                        button: PointerEventButton::Left,
                        position: current_position,
                    }
                } else {
                    WindowEvent::PointerReleased {
                        button: PointerEventButton::Left,
                        position: current_position,
                    }
                };
                window.dispatch_event(event);
            }
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, ()> for WindowEventHandler {
    fn event(
        state: &mut Self,
        layer_surface: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _data: &(),
        _conn: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                info!("Layer surface configured with size: {}x{}", width, height);
                layer_surface.ack_configure(serial);
                if let Some(state) = state.state.upgrade() {
                    let state_borrow = state.borrow();
                    if width > 0 && height > 0 {
                        state_borrow
                            .update_size(state_borrow.output_size().width, state_borrow.height());
                    } else {
                        let current_size = state_borrow.output_size();
                        state_borrow.update_size(current_size.width, current_size.height);
                    }
                }
            }
            zwlr_layer_surface_v1::Event::Closed => {
                info!("Layer surface closed");
            }
            _ => {}
        }
    }
}

impl Dispatch<WlOutput, ()> for WindowEventHandler {
    fn event(
        state: &mut Self,
        _proxy: &WlOutput,
        event: <WlOutput as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        match event {
            wl_output::Event::Mode { width, height, .. } => {
                info!("WlOutput size changed to {}x{}", width, height);
                if let Some(state) = state.state.upgrade() {
                    let state_borrow = state.borrow();
                    state_borrow.set_output_size(width as u32, height as u32);
                }
            }
            wl_output::Event::Description { ref description } => {
                info!("WlOutput description: {:?}", description);
            }
            wl_output::Event::Scale { ref factor } => {
                info!("WlOutput factor scale: {:?}", factor);
            }
            wl_output::Event::Name { ref name } => {
                info!("WlOutput name: {:?}", name);
            }
            wl_output::Event::Geometry {
                x,
                y,
                physical_width,
                physical_height,
                subpixel,
                make,
                model,
                transform,
            } => {
                info!("WlOutput geometry: x={}, y={}, physical_width={}, physical_height={}, subpixel={:?}, make={:?}, model={:?}, transform={:?}", x, y, physical_width, physical_height, subpixel, make, model, transform);
            }
            wl_output::Event::Done => {
                info!("WlOutput done");
            }
            _ => {}
        }
    }
}

impl Dispatch<WlPointer, ()> for WindowEventHandler {
    fn event(
        state: &mut Self,
        _proxy: &WlPointer,
        event: <WlPointer as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter {
                surface_x,
                surface_y,
                ..
            } => {
                state.handle_pointer_enter(surface_x, surface_y);
            }
            wl_pointer::Event::Leave { .. } => {
                state.handle_pointer_leave();
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                state.handle_pointer_motion(surface_x, surface_y);
            }
            wl_pointer::Event::Button {
                state: button_state,
                ..
            } => {
                state.handle_pointer_button(button_state);
            }
            _ => {}
        }
    }
}

impl_empty_dispatch!(
    (WlRegistry, GlobalListContents),
    (WlCompositor, ()),
    (WlSurface, ()),
    (ZwlrLayerShellV1, ()),
    (WlSeat, ())
);
