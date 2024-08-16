use crate::impl_empty_dispatch;
use log::info;
use slint::platform::{PointerEventButton, WindowEvent};
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::ZwlrLayerShellV1,
    zwlr_layer_surface_v1::{self, ZwlrLayerSurfaceV1},
};
use wayland_client::WEnum;
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
    Connection, Dispatch, Proxy, QueueHandle,
};

use super::WindowState;

impl Dispatch<ZwlrLayerSurfaceV1, ()> for WindowState {
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
                if width > 0 && height > 0 {
                    state.update_size(state.output_size().width, state.height());
                } else {
                    let current_size = state.output_size();
                    state.update_size(current_size.width, current_size.height);
                }
            }
            zwlr_layer_surface_v1::Event::Closed => {
                info!("Layer surface closed");
            }
            _ => {}
        }
    }
}

impl Dispatch<WlOutput, ()> for WindowState {
    fn event(
        state: &mut Self,
        _proxy: &WlOutput,
        event: <WlOutput as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        match event {
            wl_output::Event::Mode { width, height, .. } => {
                info!("WlOutput size changed to {}x{}", width, height);
                let width = width.try_into().unwrap_or_default();
                let height = height.try_into().unwrap_or_default();
                state.set_output_size(width, height);
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

impl Dispatch<WlPointer, ()> for WindowState {
    fn event(
        state: &mut Self,
        _proxy: &WlPointer,
        event: <WlPointer as Proxy>::Event,
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
                state.set_current_pointer_position(surface_x, surface_y);
                let logical_position = state.current_pointer_position();
                if let Some(window) = state.window() {
                    window.dispatch_event(WindowEvent::PointerMoved {
                        position: logical_position,
                    });
                }
            }
            wl_pointer::Event::Leave { .. } => {
                if let Some(window) = state.window() {
                    window.dispatch_event(WindowEvent::PointerExited);
                }
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                state.set_current_pointer_position(surface_x, surface_y);
                if let Some(window) = state.window() {
                    let logical_position = state.current_pointer_position();
                    window.dispatch_event(WindowEvent::PointerMoved {
                        position: logical_position,
                    });
                }
            }
            wl_pointer::Event::Button {
                state: button_state,
                ..
            } => {
                let is_press =
                    matches!(button_state, WEnum::Value(wl_pointer::ButtonState::Pressed));
                let current_position = state.current_pointer_position();
                if let Some(window) = state.window() {
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
