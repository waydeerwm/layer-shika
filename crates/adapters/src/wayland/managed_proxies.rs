use crate::logger;
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1;
use wayland_client::{
    protocol::{wl_keyboard::WlKeyboard, wl_pointer::WlPointer, wl_surface::WlSurface},
    Connection,
};
use wayland_protocols::wp::{
    fractional_scale::v1::client::wp_fractional_scale_v1::WpFractionalScaleV1,
    viewporter::client::wp_viewport::WpViewport,
};
use std::{ops::Deref, rc::Rc};

pub struct ManagedWlPointer {
    pointer: Rc<WlPointer>,
    connection: Rc<Connection>,
}

impl ManagedWlPointer {
    #[must_use]
    pub const fn new(pointer: Rc<WlPointer>, connection: Rc<Connection>) -> Self {
        Self {
            pointer,
            connection,
        }
    }
}

impl Deref for ManagedWlPointer {
    type Target = WlPointer;

    fn deref(&self) -> &Self::Target {
        &self.pointer
    }
}

impl Drop for ManagedWlPointer {
    fn drop(&mut self) {
        logger::debug!("Releasing WlPointer");
        self.pointer.release();
        if let Err(e) = self.connection.flush() {
            logger::error!("Failed to flush after releasing WlPointer: {e}");
        }
    }
}

pub struct ManagedWlKeyboard {
    keyboard: Rc<WlKeyboard>,
    connection: Rc<Connection>,
}

impl ManagedWlKeyboard {
    #[must_use]
    pub const fn new(keyboard: Rc<WlKeyboard>, connection: Rc<Connection>) -> Self {
        Self {
            keyboard,
            connection,
        }
    }
}

impl Deref for ManagedWlKeyboard {
    type Target = WlKeyboard;

    fn deref(&self) -> &Self::Target {
        &self.keyboard
    }
}

impl Drop for ManagedWlKeyboard {
    fn drop(&mut self) {
        logger::debug!("Releasing WlKeyboard");
        self.keyboard.release();
        if let Err(e) = self.connection.flush() {
            logger::error!("Failed to flush after releasing WlKeyboard: {e}");
        }
    }
}

pub struct ManagedWlSurface {
    surface: Rc<WlSurface>,
    connection: Rc<Connection>,
}

impl ManagedWlSurface {
    #[must_use]
    pub const fn new(surface: Rc<WlSurface>, connection: Rc<Connection>) -> Self {
        Self {
            surface,
            connection,
        }
    }

    pub const fn inner(&self) -> &Rc<WlSurface> {
        &self.surface
    }
}

impl Deref for ManagedWlSurface {
    type Target = WlSurface;

    fn deref(&self) -> &Self::Target {
        &self.surface
    }
}

impl Drop for ManagedWlSurface {
    fn drop(&mut self) {
        logger::debug!("Destroying WlSurface");
        self.surface.destroy();
        if let Err(e) = self.connection.flush() {
            logger::error!("Failed to flush after destroying WlSurface: {e}");
        }
    }
}

pub struct ManagedZwlrLayerSurfaceV1 {
    layer_surface: Rc<ZwlrLayerSurfaceV1>,
    connection: Rc<Connection>,
}

impl ManagedZwlrLayerSurfaceV1 {
    #[must_use]
    pub const fn new(layer_surface: Rc<ZwlrLayerSurfaceV1>, connection: Rc<Connection>) -> Self {
        Self {
            layer_surface,
            connection,
        }
    }

    pub const fn inner(&self) -> &Rc<ZwlrLayerSurfaceV1> {
        &self.layer_surface
    }
}

impl Deref for ManagedZwlrLayerSurfaceV1 {
    type Target = ZwlrLayerSurfaceV1;

    fn deref(&self) -> &Self::Target {
        &self.layer_surface
    }
}

impl Drop for ManagedZwlrLayerSurfaceV1 {
    fn drop(&mut self) {
        logger::debug!("Destroying ZwlrLayerSurfaceV1");
        self.layer_surface.destroy();
        if let Err(e) = self.connection.flush() {
            logger::error!("Failed to flush after destroying ZwlrLayerSurfaceV1: {e}");
        }
    }
}

pub struct ManagedWpFractionalScaleV1 {
    fractional_scale: Rc<WpFractionalScaleV1>,
    connection: Rc<Connection>,
}

impl ManagedWpFractionalScaleV1 {
    #[must_use]
    pub const fn new(
        fractional_scale: Rc<WpFractionalScaleV1>,
        connection: Rc<Connection>,
    ) -> Self {
        Self {
            fractional_scale,
            connection,
        }
    }

    pub const fn inner(&self) -> &Rc<WpFractionalScaleV1> {
        &self.fractional_scale
    }
}

impl Deref for ManagedWpFractionalScaleV1 {
    type Target = WpFractionalScaleV1;

    fn deref(&self) -> &Self::Target {
        &self.fractional_scale
    }
}

impl Drop for ManagedWpFractionalScaleV1 {
    fn drop(&mut self) {
        logger::debug!("Destroying WpFractionalScaleV1");
        self.fractional_scale.destroy();
        if let Err(e) = self.connection.flush() {
            logger::error!("Failed to flush after destroying WpFractionalScaleV1: {e}");
        }
    }
}

pub struct ManagedWpViewport {
    viewport: Rc<WpViewport>,
    connection: Rc<Connection>,
}

impl ManagedWpViewport {
    #[must_use]
    pub const fn new(viewport: Rc<WpViewport>, connection: Rc<Connection>) -> Self {
        Self {
            viewport,
            connection,
        }
    }
}

impl Deref for ManagedWpViewport {
    type Target = WpViewport;

    fn deref(&self) -> &Self::Target {
        &self.viewport
    }
}

impl Drop for ManagedWpViewport {
    fn drop(&mut self) {
        logger::debug!("Destroying WpViewport");
        self.viewport.destroy();
        if let Err(e) = self.connection.flush() {
            logger::error!("Failed to flush after destroying WpViewport: {e}");
        }
    }
}
