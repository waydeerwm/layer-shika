use crate::{
    errors::{LayerShikaError, Result},
    rendering::egl::context_factory::RenderContextFactory,
    wayland::{
        config::{LayerSurfaceConfig, WaylandSurfaceConfig},
        shell_adapter::WaylandShellSystem,
        surfaces::{
            app_state::AppState,
            event_context::SharedPointerSerial,
            layer_surface::{SurfaceCtx, SurfaceSetupParams},
            popup_manager::{PopupContext, PopupManager},
            surface_builder::SurfaceStateBuilder,
            surface_state::SurfaceState,
        },
    }, logger,
};
use layer_shika_domain::value_objects::{
    output_handle::OutputHandle,
    output_info::OutputInfo,
};
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::ZwlrLayerShellV1;
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use wayland_client::{
    backend::ObjectId,
    protocol::{wl_compositor::WlCompositor, wl_output::WlOutput, wl_pointer::WlPointer},
    Connection, Proxy, QueueHandle,
};
use wayland_protocols::{
    wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp::viewporter::client::wp_viewporter::WpViewporter,
};

use super::OutputMapping;

pub struct OutputManagerContext {
    pub compositor: WlCompositor,
    pub layer_shell: Option<ZwlrLayerShellV1>,
    pub fractional_scale_manager: Option<WpFractionalScaleManagerV1>,
    pub viewporter: Option<WpViewporter>,
    pub render_factory: Rc<RenderContextFactory>,
    pub popup_context: PopupContext,
    pub pointer: Rc<WlPointer>,
    pub shared_serial: Rc<SharedPointerSerial>,
    pub connection: Rc<Connection>,
}

impl OutputManagerContext {
    pub const fn connection(&self) -> &Rc<Connection> {
        &self.connection
    }
}

struct PendingOutput {
    proxy: WlOutput,
    #[allow(dead_code)]
    output_id: ObjectId,
    info: OutputInfo,
}

pub struct OutputManager {
    context: OutputManagerContext,
    config: WaylandSurfaceConfig,
    pub(crate) layer_surface_config: LayerSurfaceConfig,
    output_mapping: OutputMapping,
    pending_outputs: RefCell<HashMap<ObjectId, PendingOutput>>,
}

impl OutputManager {
    pub(crate) fn new(
        context: OutputManagerContext,
        config: WaylandSurfaceConfig,
        layer_surface_config: LayerSurfaceConfig,
    ) -> Self {
        Self {
            context,
            config,
            layer_surface_config,
            output_mapping: OutputMapping::new(),
            pending_outputs: RefCell::new(HashMap::new()),
        }
    }

    pub fn register_output(
        &mut self,
        output: WlOutput,
        _queue_handle: &QueueHandle<AppState>,
    ) -> OutputHandle {
        let output_id = output.id();
        let handle = self.output_mapping.insert(output_id.clone());

        logger::info!(
            "Registered new output with handle {handle:?}, id {:?}",
            output_id
        );

        let info = OutputInfo::new(handle);

        self.pending_outputs.borrow_mut().insert(
            output_id.clone(),
            PendingOutput {
                proxy: output,
                output_id,
                info,
            },
        );

        handle
    }

    pub fn finalize_output(
        &self,
        output_id: &ObjectId,
        app_state: &mut AppState,
        queue_handle: &QueueHandle<AppState>,
    ) -> Result<()> {
        let mut pending = self.pending_outputs.borrow_mut();

        let Some(pending_output) = pending.remove(output_id) else {
            return Err(LayerShikaError::InvalidInput {
                message: format!("No pending output found for id {output_id:?}"),
            });
        };

        let handle = pending_output.info.handle();
        let mut info = pending_output.info;

        let is_primary = app_state.output_registry().is_empty();
        info.set_primary(is_primary);

        if !self.config.output_policy.should_render(&info) {
            logger::info!(
                "Skipping output {:?} due to output policy (primary: {})",
                output_id,
                is_primary
            );
            return Ok(());
        }

        logger::info!(
            "Finalizing output {:?} (handle: {handle:?}, primary: {})",
            output_id,
            is_primary
        );

        let (surface, main_surface_id) =
            self.create_window_for_output(&pending_output.proxy, output_id, queue_handle)?;

        app_state.add_output(
            output_id,
            self.config.surface_handle,
            &self.config.surface_name,
            main_surface_id,
            surface,
        );

        Ok(())
    }

    fn create_window_for_output(
        &self,
        output: &WlOutput,
        _output_id: &ObjectId,
        queue_handle: &QueueHandle<AppState>,
    ) -> Result<(SurfaceState, ObjectId)> {
        let layer_shell =
            self.context
                .layer_shell
                .as_ref()
                .ok_or_else(|| LayerShikaError::InvalidInput {
                    message:
                        "wlr-layer-shell protocol not available - cannot create layer surfaces"
                            .into(),
                })?;

        let setup_params = SurfaceSetupParams {
            compositor: &self.context.compositor,
            output,
            layer_shell,
            fractional_scale_manager: self.context.fractional_scale_manager.as_ref(),
            viewporter: self.context.viewporter.as_ref(),
            queue_handle,
            layer: self.config.layer,
            namespace: self.config.namespace.clone(),
        };

        let surface_ctx = SurfaceCtx::setup(&setup_params, &self.layer_surface_config);
        let main_surface_id = surface_ctx.surface.id();

        let window = WaylandShellSystem::initialize_renderer(
            &surface_ctx.surface,
            &self.config,
            &self.context.render_factory,
        )?;

        let mut builder = SurfaceStateBuilder::new()
            .with_component_definition(self.config.component_definition.clone())
            .with_compilation_result(self.config.compilation_result.clone())
            .with_surface(Rc::clone(&surface_ctx.surface))
            .with_layer_surface(Rc::clone(&surface_ctx.layer_surface))
            .with_scale_factor(self.config.scale_factor)
            .with_height(self.config.height)
            .with_width(self.config.width)
            .with_exclusive_zone(self.config.exclusive_zone)
            .with_connection(Rc::clone(self.context.connection()))
            .with_pointer(Rc::clone(&self.context.pointer))
            .with_window(Rc::clone(&window));

        if let Some(fs) = &surface_ctx.fractional_scale {
            builder = builder.with_fractional_scale(Rc::clone(fs));
        }

        if let Some(vp) = &surface_ctx.viewport {
            builder = builder.with_viewport(Rc::clone(vp));
        }

        let mut window_state =
            SurfaceState::new(builder).map_err(|e| LayerShikaError::WindowConfiguration {
                message: e.to_string(),
            })?;

        let popup_manager = Rc::new(PopupManager::new(
            self.context.popup_context.clone(),
            Rc::clone(window_state.display_metrics()),
        ));

        window_state.set_popup_manager(Rc::clone(&popup_manager));
        window_state.set_shared_pointer_serial(Rc::clone(&self.context.shared_serial));

        Ok((window_state, main_surface_id))
    }

    pub fn remove_output(&mut self, output_id: &ObjectId, app_state: &mut AppState) {
        if let Some(handle) = self.output_mapping.remove(output_id) {
            logger::info!("Removing output {handle:?} (id: {output_id:?})");

            let removed_windows = app_state.remove_output(handle);
            if removed_windows.is_empty() {
                logger::warn!("No window found for output handle {handle:?}");
            } else {
                logger::info!(
                    "Cleaned up {} window(s) for output {handle:?}",
                    removed_windows.len()
                );
            }
        } else {
            self.pending_outputs.borrow_mut().remove(output_id);
            logger::info!("Removed pending output {output_id:?}");
        }
    }

    pub fn get_handle_by_output_id(&self, output_id: &ObjectId) -> Option<OutputHandle> {
        self.output_mapping.get(output_id)
    }

    pub fn has_pending_output(&self, output_id: &ObjectId) -> bool {
        self.pending_outputs.borrow().contains_key(output_id)
    }

    pub fn pending_output_count(&self) -> usize {
        self.pending_outputs.borrow().len()
    }
}
