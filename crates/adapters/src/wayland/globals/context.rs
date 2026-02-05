use crate::{
    bind_globals, errors::LayerShikaError,
    rendering::egl::render_context_manager::RenderContextManager,
logger};
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::ZwlrLayerShellV1;
use std::rc::Rc;
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_compositor::WlCompositor, wl_output::WlOutput, wl_seat::WlSeat},
    Connection, Proxy, QueueHandle,
};
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use wayland_protocols::wp::viewporter::client::wp_viewporter::WpViewporter;
use wayland_protocols::xdg::shell::client::xdg_wm_base::XdgWmBase;
use wayland_protocols::ext::session_lock::v1::client::ext_session_lock_manager_v1::ExtSessionLockManagerV1;

use crate::wayland::surfaces::app_state::AppState;

pub struct GlobalContext {
    pub compositor: WlCompositor,
    pub outputs: Vec<WlOutput>,
    pub layer_shell: Option<ZwlrLayerShellV1>,
    pub seat: WlSeat,
    pub xdg_wm_base: Option<XdgWmBase>,
    pub session_lock_manager: Option<ExtSessionLockManagerV1>,
    pub fractional_scale_manager: Option<WpFractionalScaleManagerV1>,
    pub viewporter: Option<WpViewporter>,
    pub render_context_manager: Rc<RenderContextManager>,
}

impl GlobalContext {
    pub fn initialize(
        connection: &Connection,
        queue_handle: &QueueHandle<AppState>,
    ) -> Result<Self, LayerShikaError> {
        let global_list = registry_queue_init::<AppState>(connection)
            .map(|(global_list, _)| global_list)
            .map_err(|e| LayerShikaError::GlobalInitialization { source: e })?;

        let (compositor, seat) = bind_globals!(
            &global_list,
            queue_handle,
            (WlCompositor, compositor, 3..=6),
            (WlSeat, seat, 1..=9)
        )?;

        let layer_shell = global_list
            .bind::<ZwlrLayerShellV1, _, _>(queue_handle, 1..=5, ())
            .ok();

        let output_names: Vec<u32> = global_list
            .contents()
            .clone_list()
            .into_iter()
            .filter(|global| global.interface == "wl_output")
            .map(|global| {
                logger::info!(
                    "Found wl_output global with name: {} at version {}",
                    global.name,
                    global.version
                );
                global.name
            })
            .collect();

        logger::info!(
            "Total unique wl_output globals found: {}",
            output_names.len()
        );

        let outputs: Vec<WlOutput> = output_names
            .iter()
            .map(|&name| {
                logger::info!("Binding wl_output with name: {}", name);
                global_list
                    .registry()
                    .bind::<WlOutput, _, _>(name, 4, queue_handle, ())
            })
            .collect();

        if outputs.is_empty() {
            return Err(LayerShikaError::InvalidInput {
                message: "No outputs found".into(),
            });
        }

        logger::info!("Discovered {} output(s)", outputs.len());

        let xdg_wm_base = global_list
            .bind::<XdgWmBase, _, _>(queue_handle, 1..=6, ())
            .ok();

        let session_lock_manager = global_list
            .bind::<ExtSessionLockManagerV1, _, _>(queue_handle, 1..=1, ())
            .ok();

        let fractional_scale_manager = global_list
            .bind::<WpFractionalScaleManagerV1, _, _>(queue_handle, 1..=1, ())
            .ok();

        let viewporter = global_list
            .bind::<WpViewporter, _, _>(queue_handle, 1..=1, ())
            .ok();

        if xdg_wm_base.is_none() {
            logger::info!("xdg-shell protocol not available, popup support disabled");
        }

        if session_lock_manager.is_none() {
            logger::info!("ext-session-lock protocol not available, session lock disabled");
        }

        if fractional_scale_manager.is_none() {
            logger::info!("Fractional scale protocol not available, using integer scaling");
        }

        if viewporter.is_none() {
            logger::info!("Viewporter protocol not available");
        }

        if layer_shell.is_none() {
            logger::info!("wlr-layer-shell protocol not available, layer surfaces disabled");
        }

        let render_context_manager = RenderContextManager::new(&connection.display().id())?;

        Ok(Self {
            compositor,
            outputs,
            layer_shell,
            seat,
            xdg_wm_base,
            session_lock_manager,
            fractional_scale_manager,
            viewporter,
            render_context_manager,
        })
    }
}
