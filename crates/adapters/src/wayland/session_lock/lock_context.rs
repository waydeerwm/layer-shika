use std::rc::Rc;

use wayland_client::QueueHandle;
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_protocols::ext::session_lock::v1::client::ext_session_lock_manager_v1::ExtSessionLockManagerV1;
use wayland_protocols::ext::session_lock::v1::client::ext_session_lock_v1::ExtSessionLockV1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use wayland_protocols::wp::viewporter::client::wp_viewporter::WpViewporter;

use crate::rendering::egl::context_factory::RenderContextFactory;
use crate::wayland::surfaces::app_state::AppState;

#[derive(Clone)]
pub struct SessionLockContext {
    compositor: WlCompositor,
    lock_manager: ExtSessionLockManagerV1,
    seat: WlSeat,
    fractional_scale_manager: Option<WpFractionalScaleManagerV1>,
    viewporter: Option<WpViewporter>,
    render_factory: Rc<RenderContextFactory>,
}

impl SessionLockContext {
    #[must_use]
    pub fn new(
        compositor: WlCompositor,
        lock_manager: ExtSessionLockManagerV1,
        seat: WlSeat,
        fractional_scale_manager: Option<WpFractionalScaleManagerV1>,
        viewporter: Option<WpViewporter>,
        render_factory: Rc<RenderContextFactory>,
    ) -> Self {
        Self {
            compositor,
            lock_manager,
            seat,
            fractional_scale_manager,
            viewporter,
            render_factory,
        }
    }

    pub const fn compositor(&self) -> &WlCompositor {
        &self.compositor
    }

    pub const fn lock_manager(&self) -> &ExtSessionLockManagerV1 {
        &self.lock_manager
    }

    pub const fn seat(&self) -> &WlSeat {
        &self.seat
    }

    pub const fn fractional_scale_manager(&self) -> Option<&WpFractionalScaleManagerV1> {
        self.fractional_scale_manager.as_ref()
    }

    pub const fn viewporter(&self) -> Option<&WpViewporter> {
        self.viewporter.as_ref()
    }

    pub const fn render_factory(&self) -> &Rc<RenderContextFactory> {
        &self.render_factory
    }
}

pub struct LockSurfaceParams<'a> {
    pub compositor: &'a WlCompositor,
    pub output: &'a WlOutput,
    pub session_lock: &'a ExtSessionLockV1,
    pub fractional_scale_manager: Option<&'a WpFractionalScaleManagerV1>,
    pub viewporter: Option<&'a WpViewporter>,
    pub queue_handle: &'a QueueHandle<AppState>,
}
