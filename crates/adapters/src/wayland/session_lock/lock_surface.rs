use std::rc::Rc;

use wayland_client::Proxy;
use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_protocols::ext::session_lock::v1::client::ext_session_lock_surface_v1::ExtSessionLockSurfaceV1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::WpFractionalScaleV1;
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;

use crate::logger;
use crate::wayland::session_lock::lock_context::LockSurfaceParams;

pub struct LockSurface {
    surface: Rc<WlSurface>,
    session_surface: Rc<ExtSessionLockSurfaceV1>,
    fractional_scale: Option<Rc<WpFractionalScaleV1>>,
    viewport: Option<Rc<WpViewport>>,
    width: u32,
    height: u32,
    configured: bool,
}

impl LockSurface {
    pub fn create(params: &LockSurfaceParams<'_>) -> Self {
        let surface = Rc::new(params.compositor.create_surface(params.queue_handle, ()));

        let session_surface = Rc::new(params.session_lock.get_lock_surface(
            &surface,
            params.output,
            params.queue_handle,
            (),
        ));

        let fractional_scale = params.fractional_scale_manager.map(|manager| {
            logger::info!("Creating fractional scale object for lock surface");
            Rc::new(manager.get_fractional_scale(&surface, params.queue_handle, ()))
        });

        let viewport = params.viewporter.map(|vp| {
            logger::info!("Creating viewport for lock surface");
            Rc::new(vp.get_viewport(&surface, params.queue_handle, ()))
        });

        surface.set_buffer_scale(1);

        Self {
            surface,
            session_surface,
            fractional_scale,
            viewport,
            width: 0,
            height: 0,
            configured: false,
        }
    }

    pub fn handle_configure(&mut self, serial: u32, width: u32, height: u32) {
        logger::info!("Lock surface configured with compositor size: {width}x{height}");
        self.session_surface.ack_configure(serial);
        self.width = width;
        self.height = height;
        self.configured = true;
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn surface_id(&self) -> ObjectId {
        self.surface.id()
    }

    #[must_use]
    pub fn lock_surface_id(&self) -> ObjectId {
        self.session_surface.id()
    }

    pub fn fractional_scale(&self) -> Option<&Rc<WpFractionalScaleV1>> {
        self.fractional_scale.as_ref()
    }

    pub const fn has_fractional_scale(&self) -> bool {
        self.fractional_scale.is_some()
    }

    pub const fn has_viewport(&self) -> bool {
        self.viewport.is_some()
    }

    pub fn configure_fractional_viewport(&self, logical_width: u32, logical_height: u32) {
        self.surface.set_buffer_scale(1);
        if let Some(vp) = &self.viewport {
            let width_i32 = i32::try_from(logical_width).unwrap_or(i32::MAX);
            let height_i32 = i32::try_from(logical_height).unwrap_or(i32::MAX);
            vp.set_destination(width_i32, height_i32);
        }
    }

    pub fn configure_buffer_scale(&self, buffer_scale: i32) {
        self.surface.set_buffer_scale(buffer_scale);
    }

    pub fn destroy(&self) {
        self.session_surface.destroy();
        self.surface.destroy();
    }
}

impl Drop for LockSurface {
    fn drop(&mut self) {
        self.destroy();
    }
}
