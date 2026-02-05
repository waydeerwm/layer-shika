pub mod callbacks;
pub mod input_handling;
pub mod lifecycle;
pub mod rendering;
pub mod state;

use std::rc::Rc;

pub use callbacks::{
    LockCallback, LockPropertyOperation, OutputFilter,
    create_lock_property_operation_with_output_filter,
};
use layer_shika_domain::prelude::OutputInfo;
use layer_shika_domain::value_objects::lock_config::LockConfig;
use layer_shika_domain::value_objects::lock_state::LockState;
use slint_interpreter::{CompilationResult, ComponentDefinition, ComponentInstance};
pub use state::{ActiveLockSurface, LockConfigureContext, LockSurfaceOutputContext};
use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::protocol::{wl_keyboard, wl_pointer};
use wayland_client::{Proxy, QueueHandle, WEnum};
use wayland_protocols::ext::session_lock::v1::client::ext_session_lock_v1::ExtSessionLockV1;

use self::input_handling::InputState;
use crate::errors::{LayerShikaError, Result};
use crate::logger;
use crate::rendering::slint_integration::platform::CustomSlintPlatform;
use crate::wayland::rendering::RenderableSet;
use crate::wayland::session_lock::lock_context::{LockSurfaceParams, SessionLockContext};
use crate::wayland::session_lock::lock_surface::LockSurface;
use crate::wayland::surfaces::app_state::AppState;
use crate::wayland::surfaces::keyboard_state::KeyboardState;

pub struct SessionLockManager {
    context: Rc<SessionLockContext>,
    session_lock: Option<ExtSessionLockV1>,
    lock_surfaces: Vec<(ObjectId, ActiveLockSurface)>,
    state: LockState,
    config: LockConfig,
    component_definition: ComponentDefinition,
    compilation_result: Option<Rc<CompilationResult>>,
    platform: Rc<CustomSlintPlatform>,
    callbacks: Vec<LockCallback>,
    property_operations: Vec<LockPropertyOperation>,
    input_state: InputState,
}

impl SessionLockManager {
    #[must_use]
    pub fn new(
        context: Rc<SessionLockContext>,
        component_definition: ComponentDefinition,
        compilation_result: Option<Rc<CompilationResult>>,
        platform: Rc<CustomSlintPlatform>,
        config: LockConfig,
    ) -> Self {
        Self {
            context,
            session_lock: None,
            lock_surfaces: Vec::new(),
            state: LockState::Inactive,
            config,
            component_definition,
            compilation_result,
            platform,
            callbacks: Vec::new(),
            property_operations: Vec::new(),
            input_state: InputState::new(),
        }
    }

    #[must_use]
    pub const fn state(&self) -> LockState {
        self.state
    }

    pub fn activate(
        &mut self,
        outputs: impl IntoIterator<Item = WlOutput>,
        queue_handle: &QueueHandle<AppState>,
    ) -> Result<()> {
        if !self.state.can_activate() {
            return Err(LayerShikaError::InvalidInput {
                message: format!("Session lock cannot activate in state {:?}", self.state),
            });
        }

        self.config.validate()?;

        let session_lock = self.context.lock_manager().lock(queue_handle, ());
        self.session_lock = Some(session_lock.clone());
        self.state = LockState::Locking;

        for output in outputs {
            let params = LockSurfaceParams {
                compositor: self.context.compositor(),
                output: &output,
                session_lock: &session_lock,
                fractional_scale_manager: self.context.fractional_scale_manager(),
                viewporter: self.context.viewporter(),
                queue_handle,
            };
            let surface = LockSurface::create(&params);
            let surface_id = surface.surface_id();
            let window = lifecycle::create_window(
                &self.context,
                &surface_id,
                self.config.scale_factor.value(),
            )?;
            self.lock_surfaces
                .push((output.id(), ActiveLockSurface::new(surface, window)));
        }

        Ok(())
    }

    pub fn handle_locked(&mut self) {
        if self.state == LockState::Locking {
            logger::info!("Session lock transitioned to Locked");
            self.state = LockState::Locked;
        }
    }

    pub fn deactivate(&mut self) -> Result<()> {
        if !self.state.can_deactivate() {
            return Err(LayerShikaError::InvalidInput {
                message: format!("Session lock cannot deactivate in state {:?}", self.state),
            });
        }

        let Some(session_lock) = self.session_lock.take() else {
            return Err(LayerShikaError::InvalidInput {
                message: "Session lock object missing during deactivate".to_string(),
            });
        };

        for (_, surface) in &self.lock_surfaces {
            surface.surface().destroy();
        }
        session_lock.unlock_and_destroy();
        self.lock_surfaces.clear();
        self.input_state.reset();
        self.state = LockState::Unlocking;
        Ok(())
    }

    pub fn handle_finished(&mut self) {
        logger::info!("Session lock finished");
        self.lock_surfaces.clear();
        self.session_lock = None;
        self.state = LockState::Inactive;
        self.input_state.reset();
    }

    pub fn add_output(
        &mut self,
        output: &WlOutput,
        queue_handle: &QueueHandle<AppState>,
    ) -> Result<()> {
        if self.state != LockState::Locked {
            return Ok(());
        }

        let output_id = output.id();
        if self.lock_surfaces.iter().any(|(id, _)| *id == output_id) {
            return Ok(());
        }

        let Some(session_lock) = self.session_lock.as_ref() else {
            return Err(LayerShikaError::InvalidInput {
                message: "Session lock object missing during output hotplug".to_string(),
            });
        };

        logger::info!("Adding lock surface for output {output_id:?}");
        let params = LockSurfaceParams {
            compositor: self.context.compositor(),
            output,
            session_lock,
            fractional_scale_manager: self.context.fractional_scale_manager(),
            viewporter: self.context.viewporter(),
            queue_handle,
        };
        let surface = LockSurface::create(&params);
        let surface_id = surface.surface_id();
        let window =
            lifecycle::create_window(&self.context, &surface_id, self.config.scale_factor.value())?;
        self.lock_surfaces
            .push((output_id, ActiveLockSurface::new(surface, window)));
        Ok(())
    }

    pub fn remove_output(&mut self, output_id: &ObjectId) {
        if let Some(pos) = self
            .lock_surfaces
            .iter()
            .position(|(id, _)| id == output_id)
        {
            let (_, surface) = self.lock_surfaces.remove(pos);
            let surface_id = surface.surface().surface_id();
            self.input_state.clear_surface_refs(&surface_id);
            drop(surface);
        }
    }

    fn find_surface_by_lock_surface_id_mut(
        &mut self,
        lock_surface_id: &ObjectId,
    ) -> Option<&mut ActiveLockSurface> {
        self.lock_surfaces
            .iter_mut()
            .find(|(_, surface)| surface.surface().lock_surface_id() == *lock_surface_id)
            .map(|(_, surface)| surface)
    }

    fn find_surface_by_surface_id(&self, surface_id: &ObjectId) -> Option<&ActiveLockSurface> {
        self.lock_surfaces
            .iter()
            .find(|(_, surface)| surface.surface().surface_id() == *surface_id)
            .map(|(_, surface)| surface)
    }

    pub fn find_output_id_for_lock_surface(&self, lock_surface_id: &ObjectId) -> Option<ObjectId> {
        self.lock_surfaces
            .iter()
            .find(|(_, surface)| surface.surface().lock_surface_id() == *lock_surface_id)
            .map(|(id, _)| id.clone())
    }

    pub fn handle_configure(
        &mut self,
        lock_surface_id: &ObjectId,
        serial: u32,
        width: u32,
        height: u32,
        output_ctx: LockSurfaceOutputContext,
    ) -> Result<()> {
        let component_name = self.component_definition.name().to_string();

        // Use output's integer scale as fallback when fractional scale isn't available
        #[allow(clippy::cast_precision_loss)]
        let scale_factor = output_ctx
            .output_info
            .as_ref()
            .and_then(OutputInfo::scale)
            .map_or(self.config.scale_factor.value(), |s| s as f32);

        let context = LockConfigureContext {
            scale_factor,
            component_definition: self.component_definition.clone(),
            compilation_result: self.compilation_result.clone(),
            platform: Rc::clone(&self.platform),
            callbacks: self.callbacks.clone(),
            property_operations: self.property_operations.clone(),
            component_name,
            output_handle: output_ctx.output_handle,
            output_info: output_ctx.output_info,
            primary_handle: output_ctx.primary_handle,
            active_handle: output_ctx.active_handle,
        };

        let Some(surface) = self.find_surface_by_lock_surface_id_mut(lock_surface_id) else {
            return Ok(());
        };

        surface.handle_configure(serial, width, height, &context)
    }

    pub fn handle_surface_configured(
        &mut self,
        lock_surface_id: &ObjectId,
        serial: u32,
        width: u32,
        height: u32,
        output_ctx: LockSurfaceOutputContext,
    ) {
        let component_name = self.component_definition.name().to_string();

        let output_scale = output_ctx.output_info.as_ref().and_then(OutputInfo::scale);
        logger::info!(
            "Lock configure: output_info present={}, output_scale={:?}",
            output_ctx.output_info.is_some(),
            output_scale
        );
        #[allow(clippy::cast_precision_loss)]
        let scale_factor = output_scale.map_or(self.config.scale_factor.value(), |s| s as f32);

        let context = LockConfigureContext {
            scale_factor,
            component_definition: self.component_definition.clone(),
            compilation_result: self.compilation_result.clone(),
            platform: Rc::clone(&self.platform),
            callbacks: self.callbacks.clone(),
            property_operations: self.property_operations.clone(),
            component_name,
            output_handle: output_ctx.output_handle,
            output_info: output_ctx.output_info,
            primary_handle: output_ctx.primary_handle,
            active_handle: output_ctx.active_handle,
        };

        let Some(surface) = self.find_surface_by_lock_surface_id_mut(lock_surface_id) else {
            return;
        };

        surface.handle_surface_configured(serial, width, height, &context);
    }

    pub fn initialize_pending_components(&mut self) -> Result<()> {
        let component_name = self.component_definition.name().to_string();

        for (_, surface) in &mut self.lock_surfaces {
            if surface.has_pending_initialization() {
                let Some(output_handle) = surface.output_handle else {
                    continue;
                };

                // Use output's integer scale as fallback when fractional scale isn't available
                #[allow(clippy::cast_precision_loss)]
                let scale_factor = surface
                    .output_info
                    .as_ref()
                    .and_then(OutputInfo::scale)
                    .map_or(self.config.scale_factor.value(), |s| s as f32);

                let context = LockConfigureContext {
                    scale_factor,
                    component_definition: self.component_definition.clone(),
                    compilation_result: self.compilation_result.clone(),
                    platform: Rc::clone(&self.platform),
                    callbacks: self.callbacks.clone(),
                    property_operations: self.property_operations.clone(),
                    component_name: component_name.clone(),
                    output_handle,
                    output_info: surface.output_info.clone(),
                    primary_handle: surface.primary_handle,
                    active_handle: surface.active_handle,
                };

                surface.initialize_pending_component(&context)?;
            }
        }

        Ok(())
    }

    pub fn render_frames(&self) -> Result<()> {
        rendering::render_frames(&self.lock_surfaces)
    }

    pub(crate) fn register_callback(&mut self, callback: LockCallback) {
        for (_, surface) in &self.lock_surfaces {
            surface.apply_callback(&callback);
        }
        self.callbacks.push(callback);
    }

    pub(crate) fn register_property_operation(
        &mut self,
        property_operation: LockPropertyOperation,
    ) {
        for (_, surface) in &self.lock_surfaces {
            surface.apply_property_operation(&property_operation);
        }
        self.property_operations.push(property_operation);
    }

    pub fn handle_fractional_scale(&mut self, fractional_scale_id: &ObjectId, scale_120ths: u32) {
        for (_, surface) in &mut self.lock_surfaces {
            let matches = surface
                .surface()
                .fractional_scale()
                .is_some_and(|fs| fs.id() == *fractional_scale_id);
            if matches {
                surface.handle_fractional_scale(scale_120ths);
            }
        }
    }

    pub fn is_lock_surface(&self, surface_id: &ObjectId) -> bool {
        self.find_surface_by_surface_id(surface_id).is_some()
    }

    pub const fn has_active_pointer(&self) -> bool {
        self.input_state.has_active_pointer()
    }

    pub const fn has_keyboard_focus(&self) -> bool {
        self.input_state.has_keyboard_focus()
    }

    pub fn handle_pointer_enter(
        &mut self,
        serial: u32,
        surface: &WlSurface,
        surface_x: f64,
        surface_y: f64,
    ) -> bool {
        input_handling::handle_pointer_enter(
            &mut self.input_state,
            &self.lock_surfaces,
            serial,
            surface,
            surface_x,
            surface_y,
        )
    }

    pub fn handle_pointer_motion(&mut self, surface_x: f64, surface_y: f64) -> bool {
        input_handling::handle_pointer_motion(
            &mut self.input_state,
            &self.lock_surfaces,
            surface_x,
            surface_y,
        )
    }

    pub fn handle_pointer_leave(&mut self) -> bool {
        input_handling::handle_pointer_leave(&mut self.input_state, &self.lock_surfaces)
    }

    pub fn handle_pointer_button(
        &mut self,
        serial: u32,
        button: u32,
        button_state: WEnum<wl_pointer::ButtonState>,
    ) -> bool {
        input_handling::handle_pointer_button(
            &mut self.input_state,
            &self.lock_surfaces,
            self.config.scale_factor.value(),
            serial,
            button,
            button_state,
        )
    }

    pub fn handle_axis_source(&mut self, axis_source: wl_pointer::AxisSource) -> bool {
        input_handling::handle_axis_source(&self.input_state, axis_source)
    }

    pub fn handle_axis(&mut self, axis: wl_pointer::Axis, value: f64) -> bool {
        input_handling::handle_axis(&mut self.input_state, axis, value)
    }

    pub fn handle_axis_discrete(&mut self, axis: wl_pointer::Axis, discrete: i32) -> bool {
        input_handling::handle_axis_discrete(&mut self.input_state, axis, discrete)
    }

    pub fn handle_axis_stop(&mut self, axis: wl_pointer::Axis) -> bool {
        input_handling::handle_axis_stop(&self.input_state, axis)
    }

    pub fn handle_pointer_frame(&mut self) -> bool {
        input_handling::handle_pointer_frame(&mut self.input_state, &self.lock_surfaces)
    }

    pub fn handle_keyboard_enter(&mut self, surface: &WlSurface) -> bool {
        input_handling::handle_keyboard_enter(&mut self.input_state, &self.lock_surfaces, surface)
    }

    pub fn handle_keyboard_leave(&mut self, surface: &WlSurface) -> bool {
        input_handling::handle_keyboard_leave(&mut self.input_state, surface)
    }

    pub fn handle_keyboard_key(
        &mut self,
        key: u32,
        state: wl_keyboard::KeyState,
        keyboard_state: &mut KeyboardState,
    ) -> bool {
        input_handling::handle_keyboard_key(
            &self.input_state,
            &self.lock_surfaces,
            key,
            state,
            keyboard_state,
        )
    }

    pub fn iter_lock_surfaces(&self, f: &mut dyn FnMut(&ObjectId, &ComponentInstance)) {
        for (output_id, active_surface) in &self.lock_surfaces {
            if let Some(component) = active_surface.component() {
                f(output_id, component.component_instance());
            }
        }
    }

    pub const fn component_name(&self) -> &ComponentDefinition {
        &self.component_definition
    }

    pub fn count_lock_surfaces(&self) -> usize {
        self.lock_surfaces
            .iter()
            .filter(|(_, s)| s.component().is_some())
            .count()
    }
}

impl RenderableSet for SessionLockManager {
    fn render_all_dirty(&self) -> Result<()> {
        rendering::render_frames(&self.lock_surfaces)
    }
}
