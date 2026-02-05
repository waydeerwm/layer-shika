use crate::wayland::surfaces::keyboard_state::{KeyboardState, keysym_to_text};
use crate::wayland::surfaces::pointer_utils::wayland_button_to_slint;
use crate::wayland::surfaces::surface_state::SurfaceState;
use crate::logger;
use slint::{
    PhysicalSize,
    platform::WindowEvent,
};
use slint::SharedString;
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
};
use wayland_client::backend::ObjectId;
use wayland_client::WEnum;
use wayland_client::{
    Proxy,
    protocol::{
        wl_keyboard,
        wl_pointer,
        wl_surface::WlSurface,
    },
};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_v1::WpFractionalScaleV1,
};
use wayland_protocols::xdg::shell::client::{
    xdg_popup::XdgPopup,
    xdg_surface::XdgSurface,
    xdg_wm_base::XdgWmBase,
};
use xkbcommon::xkb;

impl SurfaceState {
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn handle_layer_surface_configure(
        &mut self,
        layer_surface: &ZwlrLayerSurfaceV1,
        serial: u32,
        width: u32,
        height: u32,
    ) {
        logger::info!("Layer surface configured with compositor size: {width}x{height}");
        layer_surface.ack_configure(serial);

        let output_width = self.output_size().width;
        let scale_factor = self.scale_factor();

        let target_width = if width == 0 || (width == 1 && output_width > 1) {
            if scale_factor > 1.0 {
                (output_width as f32 / scale_factor).round() as u32
            } else {
                output_width
            }
        } else {
            width
        };

        let target_height = if height > 0 {
            height
        } else {
            let h = self.height();
            if scale_factor > 1.0 {
                (h as f32 / scale_factor).round() as u32
            } else {
                h
            }
        };

        let clamped_width = target_width.min(output_width);

        logger::info!(
            "Using dimensions: {}x{} (clamped from {}x{}, output: {}x{})",
            clamped_width,
            target_height,
            target_width,
            target_height,
            output_width,
            self.output_size().height
        );

        self.update_size(clamped_width, target_height);
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn handle_layer_surface_closed(&mut self) {
        logger::info!("Layer surface closed");
    }

    pub(crate) fn handle_output_mode(&mut self, width: i32, height: i32) {
        logger::info!("WlOutput size changed to {width}x{height}");
        let width = width.try_into().unwrap_or_default();
        let height = height.try_into().unwrap_or_default();
        self.set_output_size(PhysicalSize::new(width, height));
    }

    pub(crate) fn handle_pointer_enter(
        &mut self,
        serial: u32,
        surface: &WlSurface,
        surface_x: f64,
        surface_y: f64,
    ) {
        self.set_last_pointer_serial(serial);
        self.set_current_pointer_position(surface_x, surface_y);

        self.set_entered_surface(surface);
        let position = self.current_pointer_position();

        self.dispatch_to_active_window(WindowEvent::PointerMoved { position });
    }

    pub(crate) fn handle_pointer_motion(&mut self, surface_x: f64, surface_y: f64) {
        self.set_current_pointer_position(surface_x, surface_y);
        let position = self.current_pointer_position();

        self.dispatch_to_active_window(WindowEvent::PointerMoved { position });
    }

    pub(crate) fn handle_pointer_leave(&mut self) {
        self.dispatch_to_active_window(WindowEvent::PointerExited);
        self.clear_entered_surface();
    }

    pub(crate) fn handle_pointer_button(
        &mut self,
        serial: u32,
        button: u32,
        button_state: WEnum<wl_pointer::ButtonState>,
    ) {
        self.set_last_pointer_serial(serial);
        let position = self.current_pointer_position();
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
            _ => return,
        };

        self.dispatch_to_active_window(event);
    }

    pub(crate) fn handle_axis_source(&mut self, axis_source: wl_pointer::AxisSource) {
        self.set_axis_source(axis_source);
    }

    pub(crate) fn handle_axis(&mut self, _time: u32, axis: wl_pointer::Axis, value: f64) {
        self.accumulate_axis(axis, value);
    }

    pub(crate) fn handle_axis_discrete(&mut self, axis: wl_pointer::Axis, discrete: i32) {
        self.accumulate_axis_discrete(axis, discrete);
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn handle_axis_stop(&mut self, _time: u32, _axis: wl_pointer::Axis) {}

    pub(crate) fn handle_pointer_frame(&mut self) {
        let (delta_x, delta_y) = self.take_accumulated_axis();

        if delta_x.abs() > f32::EPSILON || delta_y.abs() > f32::EPSILON {
            let position = self.current_pointer_position();
            self.dispatch_to_active_window(WindowEvent::PointerScrolled {
                position,
                delta_x,
                delta_y,
            });
        }
    }

    pub(crate) fn handle_keyboard_key(
        &mut self,
        surface_id: &ObjectId,
        key: u32,
        state: wl_keyboard::KeyState,
        keyboard_state: &mut KeyboardState,
    ) {
        let Some(xkb_state) = keyboard_state.xkb_state.as_mut() else {
            return;
        };

        let keycode = xkb::Keycode::new(key + 8);
        let direction = match state {
            wl_keyboard::KeyState::Pressed => xkb::KeyDirection::Down,
            wl_keyboard::KeyState::Released => xkb::KeyDirection::Up,
            _ => return,
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
            return;
        };

        let event = match state {
            wl_keyboard::KeyState::Pressed => WindowEvent::KeyPressed { text },
            wl_keyboard::KeyState::Released => WindowEvent::KeyReleased { text },
            _ => return,
        };

        self.dispatch_to_surface(surface_id, event);
    }

    pub(crate) fn handle_fractional_scale(&mut self, proxy: &WpFractionalScaleV1, scale: u32) {
        use crate::wayland::surfaces::display_metrics::DisplayMetrics;
        let scale_float = DisplayMetrics::scale_factor_from_120ths(scale);
        logger::info!("Fractional scale received: {scale_float} ({scale}x)");
        self.update_scale_for_fractional_scale_object(proxy, scale);
    }

    pub(crate) fn handle_xdg_popup_configure(
        &mut self,
        xdg_popup: &XdgPopup,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) {
        logger::info!("XdgPopup Configure: position=({x}, {y}), size=({width}x{height})");

        if let Some(popup_manager) = self.popup_manager() {
            let popup_id = xdg_popup.id();
            if let Some(handle) = popup_manager.find_by_xdg_popup(&popup_id) {
                logger::info!(
                    "Marking popup with handle {handle:?} as configured after XdgPopup::Configure"
                );
                popup_manager.mark_popup_configured(handle.key());
                popup_manager.commit_popup_surface(handle.key());
                popup_manager.mark_all_popups_dirty();
            }
        }
    }

    pub(crate) fn handle_xdg_popup_done(&mut self, xdg_popup: &XdgPopup) {
        logger::info!("XdgPopup dismissed by compositor");
        let popup_id = xdg_popup.id();
        let popup_handle = self
            .popup_manager()
            .as_ref()
            .and_then(|pm| pm.find_by_xdg_popup(&popup_id));

        if let Some(handle) = popup_handle {
            logger::info!("Destroying popup with handle {handle:?}");
            if let Some(popup_manager) = self.popup_manager() {
                let _result = popup_manager.close(handle);
            }
        }
    }

    pub(crate) fn handle_xdg_surface_configure(&mut self, xdg_surface: &XdgSurface, serial: u32) {
        logger::info!("XdgSurface Configure received, sending ack with serial {serial}");
        xdg_surface.ack_configure(serial);

        if let Some(popup_manager) = self.popup_manager() {
            logger::info!("Marking all popups as dirty after Configure");
            popup_manager.mark_all_popups_dirty();
        }
    }

    pub(crate) fn handle_xdg_wm_base_ping(xdg_wm_base: &XdgWmBase, serial: u32) {
        logger::info!("XdgWmBase ping received, sending pong with serial {serial}");
        xdg_wm_base.pong(serial);
    }
}
