use std::rc::Rc;

use layer_shika_domain::surface_dimensions::SurfaceDimensions;
use layer_shika_domain::value_objects::output_handle::OutputHandle;
use layer_shika_domain::value_objects::output_info::OutputInfo;
use slint::platform::{WindowAdapter, WindowEvent};
use slint::{LogicalPosition, LogicalSize, WindowSize};
use slint_interpreter::{CompilationResult, ComponentDefinition};

use super::callbacks::{
    LockCallback, LockCallbackContext, LockCallbackExt, LockPropertyOperationExt,
};
use crate::errors::Result;
use crate::logger;
use crate::rendering::femtovg::main_window::FemtoVGWindow;
use crate::rendering::femtovg::renderable_window::{FractionalScaleConfig, RenderableWindow};
use crate::rendering::slint_integration::platform::CustomSlintPlatform;
use crate::wayland::session_lock::lock_surface::LockSurface;
use crate::wayland::surfaces::component_state::ComponentState;
use crate::wayland::surfaces::display_metrics::DisplayMetrics;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockScalingMode {
    FractionalWithViewport,
    FractionalOnly,
    Integer,
}

pub struct LockSurfaceOutputContext {
    pub output_handle: OutputHandle,
    pub output_info: Option<OutputInfo>,
    pub primary_handle: Option<OutputHandle>,
    pub active_handle: Option<OutputHandle>,
}

pub struct LockConfigureContext {
    pub scale_factor: f32,
    pub component_definition: ComponentDefinition,
    pub compilation_result: Option<Rc<CompilationResult>>,
    pub platform: Rc<CustomSlintPlatform>,
    pub callbacks: Vec<LockCallback>,
    pub property_operations: Vec<super::callbacks::LockPropertyOperation>,
    pub component_name: String,
    pub output_handle: OutputHandle,
    pub output_info: Option<OutputInfo>,
    pub primary_handle: Option<OutputHandle>,
    pub active_handle: Option<OutputHandle>,
}

pub struct ActiveLockSurface {
    surface: LockSurface,
    window: Rc<FemtoVGWindow>,
    component: Option<ComponentState>,
    scale_factor: f32,
    has_fractional_scale: bool,
    component_name: Option<String>,
    pub(super) output_handle: Option<OutputHandle>,
    pub(super) output_info: Option<OutputInfo>,
    pub(super) primary_handle: Option<OutputHandle>,
    pub(super) active_handle: Option<OutputHandle>,
    pending_component_initialization: bool,
}

impl ActiveLockSurface {
    pub fn new(surface: LockSurface, window: Rc<FemtoVGWindow>) -> Self {
        Self {
            has_fractional_scale: surface.fractional_scale().is_some(),
            surface,
            window,
            component: None,
            scale_factor: 1.0,
            output_handle: None,
            component_name: None,
            output_info: None,
            primary_handle: None,
            active_handle: None,
            pending_component_initialization: false,
        }
    }

    pub fn handle_surface_configured(
        &mut self,
        serial: u32,
        width: u32,
        height: u32,
        context: &LockConfigureContext,
    ) {
        self.surface.handle_configure(serial, width, height);
        self.scale_factor = context.scale_factor;
        self.output_handle = Some(context.output_handle);
        self.component_name = Some(context.component_name.clone());
        self.output_info.clone_from(&context.output_info);
        self.primary_handle = context.primary_handle;
        self.active_handle = context.active_handle;
        let dimensions = match SurfaceDimensions::calculate(width, height, context.scale_factor) {
            Ok(dimensions) => dimensions,
            Err(err) => {
                logger::info!("Failed to calculate lock surface dimensions: {err}");
                return;
            }
        };
        let scaling_mode = self.scaling_mode();
        logger::info!(
            "Lock surface dimensions: logical {}x{}, physical {}x{}, scale {}, mode {:?}",
            dimensions.logical_width(),
            dimensions.logical_height(),
            dimensions.physical_width(),
            dimensions.physical_height(),
            context.scale_factor,
            scaling_mode
        );
        self.configure_window(&dimensions, scaling_mode, context.scale_factor);
        self.configure_surface(&dimensions, scaling_mode);

        if self.component.is_none() {
            self.pending_component_initialization = true;
        }

        RenderableWindow::request_redraw(self.window.as_ref());
    }

    pub fn handle_configure(
        &mut self,
        serial: u32,
        width: u32,
        height: u32,
        context: &LockConfigureContext,
    ) -> Result<()> {
        self.handle_surface_configured(serial, width, height, context);

        if self.pending_component_initialization {
            self.initialize_component(context)?;
        }

        Ok(())
    }

    fn initialize_component(&mut self, context: &LockConfigureContext) -> Result<()> {
        if self.component.is_some() {
            return Ok(());
        }

        context.platform.add_window(Rc::clone(&self.window));
        let component = ComponentState::new(
            context.component_definition.clone(),
            context.compilation_result.clone(),
            &self.window,
        )?;
        self.window
            .window()
            .dispatch_event(WindowEvent::WindowActiveChanged(true));

        let callback_context = LockCallbackContext::new(
            context.component_name.clone(),
            context.output_handle,
            context.output_info.clone(),
            context.primary_handle,
            context.active_handle,
        );

        for callback in &context.callbacks {
            if let Err(err) =
                callback.apply_with_context(component.component_instance(), &callback_context)
            {
                logger::info!(
                    "Failed to register lock callback '{}': {err}",
                    callback.name()
                );
            } else if callback.should_apply(&callback_context) {
                logger::info!("Registered lock callback '{}'", callback.name());
            } else {
                logger::info!(
                    "Skipping callback '{}' due to selector filter (output {:?})",
                    callback.name(),
                    context.output_handle
                );
            }
        }

        for property_op in &context.property_operations {
            if property_op.should_apply(&callback_context) {
                if let Err(err) = property_op.apply_to_component(component.component_instance()) {
                    logger::info!(
                        "Failed to set lock property '{}': {err}",
                        property_op.name()
                    );
                } else {
                    logger::info!(
                        "Set lock property '{}' on output {:?}",
                        property_op.name(),
                        context.output_handle
                    );
                }
            } else {
                logger::info!(
                    "Skipping property '{}' due to selector filter (output {:?}, primary={:?})",
                    property_op.name(),
                    context.output_handle,
                    context.primary_handle
                );
            }
        }

        self.component = Some(component);
        self.pending_component_initialization = false;

        Ok(())
    }

    pub fn render_frame_if_dirty(&self) -> Result<()> {
        self.window.render_frame_if_dirty()
    }

    pub fn handle_fractional_scale(&mut self, scale_120ths: u32) {
        let scale_factor = DisplayMetrics::scale_factor_from_120ths(scale_120ths);
        self.scale_factor = scale_factor;
        if self.surface.width() == 0 || self.surface.height() == 0 {
            return;
        }
        let Ok(dimensions) =
            SurfaceDimensions::calculate(self.surface.width(), self.surface.height(), scale_factor)
        else {
            return;
        };
        let scaling_mode = self.scaling_mode();
        self.configure_window(&dimensions, scaling_mode, scale_factor);
        self.configure_surface(&dimensions, scaling_mode);
        RenderableWindow::request_redraw(self.window.as_ref());
    }

    pub fn apply_callback(&self, callback: &LockCallback) {
        let Some(component) = self.component.as_ref() else {
            return;
        };

        let Some(component_name) = &self.component_name else {
            return;
        };

        let Some(output_handle) = self.output_handle else {
            return;
        };

        let callback_context = LockCallbackContext::new(
            component_name.clone(),
            output_handle,
            self.output_info.clone(),
            self.primary_handle,
            self.active_handle,
        );

        if let Err(err) =
            callback.apply_with_context(component.component_instance(), &callback_context)
        {
            logger::info!(
                "Failed to register lock callback '{}': {err}",
                callback.name()
            );
        }
    }

    pub fn apply_property_operation(&self, property_op: &super::callbacks::LockPropertyOperation) {
        let Some(component) = self.component.as_ref() else {
            return;
        };

        let Some(component_name) = &self.component_name else {
            return;
        };

        let Some(output_handle) = self.output_handle else {
            return;
        };

        let callback_context = LockCallbackContext::new(
            component_name.clone(),
            output_handle,
            self.output_info.clone(),
            self.primary_handle,
            self.active_handle,
        );

        if let Err(err) =
            property_op.apply_with_context(component.component_instance(), &callback_context)
        {
            logger::info!(
                "Failed to set lock property '{}': {err}",
                property_op.name()
            );
        }
    }

    fn scaling_mode(&self) -> LockScalingMode {
        if self.surface.has_fractional_scale() && self.surface.has_viewport() {
            LockScalingMode::FractionalWithViewport
        } else if self.surface.has_fractional_scale() {
            LockScalingMode::FractionalOnly
        } else {
            LockScalingMode::Integer
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn configure_window(
        &self,
        dimensions: &SurfaceDimensions,
        mode: LockScalingMode,
        scale_factor: f32,
    ) {
        match mode {
            LockScalingMode::FractionalWithViewport => {
                let config = FractionalScaleConfig::new(
                    dimensions.logical_width() as f32,
                    dimensions.logical_height() as f32,
                    scale_factor,
                );
                logger::info!(
                    "Lock FractionalWithViewport: render scale {} (from {}), physical {}x{}",
                    config.render_scale,
                    scale_factor,
                    config.render_physical_size.width,
                    config.render_physical_size.height
                );
                config.apply_to(self.window.as_ref());
            }
            LockScalingMode::FractionalOnly => {
                RenderableWindow::set_scale_factor(self.window.as_ref(), scale_factor);
                self.window.set_size(WindowSize::Logical(LogicalSize::new(
                    dimensions.logical_width() as f32,
                    dimensions.logical_height() as f32,
                )));
            }
            LockScalingMode::Integer => {
                RenderableWindow::set_scale_factor(self.window.as_ref(), scale_factor);
                self.window
                    .set_size(WindowSize::Physical(slint::PhysicalSize::new(
                        dimensions.physical_width(),
                        dimensions.physical_height(),
                    )));
            }
        }
    }

    fn configure_surface(&self, dimensions: &SurfaceDimensions, mode: LockScalingMode) {
        match mode {
            LockScalingMode::FractionalWithViewport => {
                self.surface.configure_fractional_viewport(
                    dimensions.logical_width(),
                    dimensions.logical_height(),
                );
            }
            LockScalingMode::FractionalOnly | LockScalingMode::Integer => {
                self.surface
                    .configure_buffer_scale(dimensions.buffer_scale());
            }
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn to_logical_position(&self, surface_x: f64, surface_y: f64) -> LogicalPosition {
        if self.has_fractional_scale {
            let x = surface_x as f32;
            let y = surface_y as f32;
            LogicalPosition::new(x, y)
        } else {
            let x = (surface_x / f64::from(self.scale_factor)) as f32;
            let y = (surface_y / f64::from(self.scale_factor)) as f32;
            LogicalPosition::new(x, y)
        }
    }

    pub fn dispatch_event(&self, event: WindowEvent) {
        self.window.window().dispatch_event(event);
    }

    pub const fn surface(&self) -> &LockSurface {
        &self.surface
    }

    pub const fn component(&self) -> Option<&ComponentState> {
        self.component.as_ref()
    }

    pub const fn has_pending_initialization(&self) -> bool {
        self.pending_component_initialization
    }

    pub fn initialize_pending_component(&mut self, context: &LockConfigureContext) -> Result<()> {
        if self.pending_component_initialization {
            self.initialize_component(context)?;
        }
        Ok(())
    }
}
