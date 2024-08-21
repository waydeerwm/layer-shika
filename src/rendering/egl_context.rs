use crate::errors::LayerShikaError;
use glutin::{
    api::egl::{context::PossiblyCurrentContext, display::Display, surface::Surface},
    config::ConfigTemplateBuilder,
    context::ContextAttributesBuilder,
    display::GetGlDisplay,
    prelude::*,
    surface::{SurfaceAttributesBuilder, WindowSurface},
};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use slint::{platform::femtovg_renderer::OpenGLInterface, PhysicalSize};
use std::{
    ffi::{self, c_void, CStr},
    num::NonZeroU32,
    ptr::NonNull,
};
use wayland_client::backend::ObjectId;

pub struct EGLContext {
    context: PossiblyCurrentContext,
    surface: Surface<WindowSurface>,
}

#[derive(Default)]
pub struct EGLContextBuilder {
    display_id: Option<ObjectId>,
    surface_id: Option<ObjectId>,
    size: Option<PhysicalSize>,
    config_template: Option<ConfigTemplateBuilder>,
    context_attributes: Option<ContextAttributesBuilder>,
}

impl EGLContextBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_display_id(mut self, display_id: ObjectId) -> Self {
        self.display_id = Some(display_id);
        self
    }

    pub fn with_surface_id(mut self, surface_id: ObjectId) -> Self {
        self.surface_id = Some(surface_id);
        self
    }

    pub const fn with_size(mut self, size: PhysicalSize) -> Self {
        self.size = Some(size);
        self
    }

    #[allow(dead_code)]
    pub const fn with_config_template(mut self, config_template: ConfigTemplateBuilder) -> Self {
        self.config_template = Some(config_template);
        self
    }

    #[allow(dead_code)]
    pub const fn with_context_attributes(
        mut self,
        context_attributes: ContextAttributesBuilder,
    ) -> Self {
        self.context_attributes = Some(context_attributes);
        self
    }

    pub fn build(self) -> Result<EGLContext, LayerShikaError> {
        let display_id = self
            .display_id
            .ok_or_else(|| LayerShikaError::InvalidInput("Display ID is required".into()))?;
        let surface_id = self
            .surface_id
            .ok_or_else(|| LayerShikaError::InvalidInput("Surface ID is required".into()))?;
        let size = self
            .size
            .ok_or_else(|| LayerShikaError::InvalidInput("Size is required".into()))?;

        let display_handle = create_wayland_display_handle(&display_id)?;
        let glutin_display = unsafe { Display::new(display_handle) }.map_err(|e| {
            LayerShikaError::EGLContextCreation(format!("Failed to create display: {e}"))
        })?;

        let config_template = self.config_template.unwrap_or_default();

        let config = select_config(&glutin_display, config_template)?;

        let context_attributes = self.context_attributes.unwrap_or_default();

        let context = create_context(&glutin_display, &config, context_attributes)?;

        let surface_handle = create_surface_handle(&surface_id)?;
        let surface = create_surface(&glutin_display, &config, surface_handle, size)?;

        let context = context
            .make_current(&surface)
            .map_err(|e| LayerShikaError::EGLContextCreation(format!("Unable to activate EGL context: {e}. This may indicate a problem with the graphics drivers.")))?;

        Ok(EGLContext { context, surface })
    }
}

impl EGLContext {
    pub fn builder() -> EGLContextBuilder {
        EGLContextBuilder::new()
    }

    fn ensure_current(&self) -> Result<(), LayerShikaError> {
        if !self.context.is_current() {
            self.context.make_current(&self.surface).map_err(|e| {
                LayerShikaError::EGLContextCreation(format!("Failed to make context current: {e}"))
            })?;
        }
        Ok(())
    }
}

fn create_wayland_display_handle(
    display_id: &ObjectId,
) -> Result<RawDisplayHandle, LayerShikaError> {
    let display = NonNull::new(display_id.as_ptr().cast::<c_void>()).ok_or_else(|| {
        LayerShikaError::InvalidInput("Failed to create NonNull pointer for display".into())
    })?;
    let handle = WaylandDisplayHandle::new(display);
    Ok(RawDisplayHandle::Wayland(handle))
}

fn select_config(
    glutin_display: &Display,
    config_template: ConfigTemplateBuilder,
) -> Result<glutin::api::egl::config::Config, LayerShikaError> {
    let mut configs = unsafe { glutin_display.find_configs(config_template.build()) }
        .map_err(|e| LayerShikaError::EGLContextCreation(format!("Failed to find configs: {e}")))?;
    configs.next().ok_or_else(|| {
        LayerShikaError::EGLContextCreation("No compatible EGL configurations found.".into())
    })
}

fn create_context(
    glutin_display: &Display,
    config: &glutin::api::egl::config::Config,
    context_attributes: ContextAttributesBuilder,
) -> Result<glutin::api::egl::context::NotCurrentContext, LayerShikaError> {
    unsafe { glutin_display.create_context(config, &context_attributes.build(None)) }
        .map_err(|e| LayerShikaError::EGLContextCreation(format!("Failed to create context: {e}")))
}

fn create_surface_handle(surface_id: &ObjectId) -> Result<RawWindowHandle, LayerShikaError> {
    let surface = NonNull::new(surface_id.as_ptr().cast::<c_void>()).ok_or_else(|| {
        LayerShikaError::InvalidInput("Failed to create NonNull pointer for surface".into())
    })?;
    let handle = WaylandWindowHandle::new(surface);
    Ok(RawWindowHandle::Wayland(handle))
}

fn create_surface(
    glutin_display: &Display,
    config: &glutin::api::egl::config::Config,
    surface_handle: RawWindowHandle,
    size: PhysicalSize,
) -> Result<Surface<WindowSurface>, LayerShikaError> {
    let width = NonZeroU32::new(size.width)
        .ok_or_else(|| LayerShikaError::InvalidInput("Width cannot be zero".into()))?;

    let height = NonZeroU32::new(size.height)
        .ok_or_else(|| LayerShikaError::InvalidInput("Height cannot be zero".into()))?;

    let attrs =
        SurfaceAttributesBuilder::<WindowSurface>::new().build(surface_handle, width, height);

    unsafe { glutin_display.create_window_surface(config, &attrs) }.map_err(|e| {
        LayerShikaError::EGLContextCreation(format!("Failed to create window surface: {e}"))
    })
}

unsafe impl OpenGLInterface for EGLContext {
    fn ensure_current(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.ensure_current()
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    fn swap_buffers(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.surface.swap_buffers(&self.context).map_err(|e| {
            LayerShikaError::EGLContextCreation(format!("Failed to swap buffers: {e}")).into()
        })
    }

    fn resize(
        &self,
        width: NonZeroU32,
        height: NonZeroU32,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.ensure_current()?;
        self.surface.resize(&self.context, width, height);
        Ok(())
    }

    fn get_proc_address(&self, name: &CStr) -> *const ffi::c_void {
        self.context.display().get_proc_address(name)
    }
}
