use anyhow::{anyhow, Result};
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

#[allow(dead_code)]
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

    pub const fn with_config_template(mut self, config_template: ConfigTemplateBuilder) -> Self {
        self.config_template = Some(config_template);
        self
    }

    pub const fn with_context_attributes(
        mut self,
        context_attributes: ContextAttributesBuilder,
    ) -> Self {
        self.context_attributes = Some(context_attributes);
        self
    }

    pub fn build(self) -> Result<EGLContext> {
        let display_id = self
            .display_id
            .ok_or_else(|| anyhow!("Display ID is required"))?;
        let surface_id = self
            .surface_id
            .ok_or_else(|| anyhow!("Surface ID is required"))?;
        let size = self.size.ok_or_else(|| anyhow!("Size is required"))?;

        let display_handle = create_wayland_display_handle(&display_id)?;
        let glutin_display = unsafe { Display::new(display_handle) }?;

        let config_template = self.config_template.unwrap_or_default();

        let config = select_config(&glutin_display, config_template)?;

        let context_attributes = self.context_attributes.unwrap_or_default();

        let context = create_context(&glutin_display, &config, context_attributes)?;

        let surface_handle = create_surface_handle(&surface_id)?;
        let surface = create_surface(&glutin_display, &config, surface_handle, size)?;

        let context = context
            .make_current(&surface)
            .map_err(|e| anyhow!("Unable to activate EGL context: {}. This may indicate a problem with the graphics drivers.", e))?;

        Ok(EGLContext { context, surface })
    }
}

impl EGLContext {
    pub fn builder() -> EGLContextBuilder {
        EGLContextBuilder::new()
    }

    fn ensure_current(&self) -> Result<()> {
        if !self.context.is_current() {
            self.context.make_current(&self.surface)?;
        }
        Ok(())
    }
}

fn create_wayland_display_handle(display_id: &ObjectId) -> Result<RawDisplayHandle> {
    let display = NonNull::new(display_id.as_ptr().cast::<c_void>())
        .ok_or_else(|| anyhow!("Failed to create NonNull pointer for display"))?;
    let handle = WaylandDisplayHandle::new(display);
    Ok(RawDisplayHandle::Wayland(handle))
}

fn select_config(
    glutin_display: &Display,
    config_template: ConfigTemplateBuilder,
) -> Result<glutin::api::egl::config::Config> {
    let mut configs = unsafe { glutin_display.find_configs(config_template.build()) }?;
    configs
        .next()
        .ok_or_else(|| anyhow!("No compatible EGL configurations found."))
}
fn create_context(
    glutin_display: &Display,
    config: &glutin::api::egl::config::Config,
    context_attributes: ContextAttributesBuilder,
) -> Result<glutin::api::egl::context::NotCurrentContext> {
    unsafe { glutin_display.create_context(config, &context_attributes.build(None)) }
        .map_err(|e| anyhow!("Failed to create context: {}", e))
}

fn create_surface_handle(surface_id: &ObjectId) -> Result<RawWindowHandle> {
    let surface = NonNull::new(surface_id.as_ptr().cast::<c_void>())
        .ok_or_else(|| anyhow!("Failed to create NonNull pointer for surface"))?;
    let handle = WaylandWindowHandle::new(surface);
    Ok(RawWindowHandle::Wayland(handle))
}

fn create_surface(
    glutin_display: &Display,
    config: &glutin::api::egl::config::Config,
    surface_handle: RawWindowHandle,
    size: PhysicalSize,
) -> Result<Surface<WindowSurface>> {
    let Some(width) = NonZeroU32::new(size.width) else {
        return Err(anyhow!("Width cannot be zero"));
    };

    let Some(height) = NonZeroU32::new(size.height) else {
        return Err(anyhow!("Height cannot be zero"));
    };

    let attrs =
        SurfaceAttributesBuilder::<WindowSurface>::new().build(surface_handle, width, height);

    unsafe { glutin_display.create_window_surface(config, &attrs) }
        .map_err(|e| anyhow!("Failed to create window surface: {}", e))
}

unsafe impl OpenGLInterface for EGLContext {
    fn ensure_current(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.ensure_current()?)
    }

    fn swap_buffers(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.surface
            .swap_buffers(&self.context)
            .map_err(|e| anyhow!("Failed to swap buffers: {}", e))
            .map_err(Into::into)
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
