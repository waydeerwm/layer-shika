use std::error::Error;
use std::ffi::{self, CStr, c_void};
use std::num::NonZeroU32;
use std::ptr::NonNull;
use std::result::Result as StdResult;

use glutin::api::egl::config::Config;
use glutin::api::egl::context::{NotCurrentContext, PossiblyCurrentContext};
use glutin::api::egl::display::Display;
use glutin::api::egl::surface::Surface;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::ContextAttributesBuilder;
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{SurfaceAttributesBuilder, WindowSurface};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use slint::PhysicalSize;
use slint::platform::femtovg_renderer::OpenGLInterface;
use wayland_client::backend::ObjectId;

use crate::errors::{EGLError, LayerShikaError, Result};
use crate::logger;

pub struct EGLContext {
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
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
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn display_id(mut self, display_id: ObjectId) -> Self {
        self.display_id = Some(display_id);
        self
    }

    #[must_use]
    pub fn surface_id(mut self, surface_id: ObjectId) -> Self {
        self.surface_id = Some(surface_id);
        self
    }

    #[must_use]
    pub const fn size(mut self, size: PhysicalSize) -> Self {
        self.size = Some(size);
        self
    }

    pub fn build(self) -> Result<EGLContext> {
        let display_id = self
            .display_id
            .ok_or_else(|| LayerShikaError::InvalidInput {
                message: "Display ID is required".into(),
            })?;
        let surface_id = self
            .surface_id
            .ok_or_else(|| LayerShikaError::InvalidInput {
                message: "Surface ID is required".into(),
            })?;
        let size = self.size.ok_or_else(|| LayerShikaError::InvalidInput {
            message: "Size is required".into(),
        })?;

        let display_handle = create_wayland_display_handle(&display_id)?;
        let glutin_display = unsafe { Display::new(display_handle) }
            .map_err(|e| EGLError::DisplayCreation { source: e.into() })?;

        let config_template = self.config_template.unwrap_or_default();

        let config = select_config(&glutin_display, config_template)?;

        let context_attributes = self.context_attributes.unwrap_or_default();

        let context = create_context(&glutin_display, &config, context_attributes)?;

        let surface_handle = create_surface_handle(&surface_id)?;
        let surface = create_surface(&glutin_display, &config, surface_handle, size)?;

        let context = context
            .make_current(&surface)
            .map_err(|e| EGLError::MakeCurrent { source: e.into() })?;

        Ok(EGLContext { surface, context })
    }
}

impl EGLContext {
    #[must_use]
    pub fn builder() -> EGLContextBuilder {
        EGLContextBuilder::new()
    }

    #[must_use]
    pub(super) fn from_raw(
        surface: Surface<WindowSurface>,
        context: PossiblyCurrentContext,
    ) -> Self {
        Self { surface, context }
    }

    fn ensure_current(&self) -> Result<()> {
        if !self.context.is_current() {
            self.context
                .make_current(&self.surface)
                .map_err(|e| EGLError::MakeCurrent { source: e.into() })?;
        }
        Ok(())
    }
}

impl Drop for EGLContext {
    fn drop(&mut self) {
        if self.context.is_current() {
            if let Err(e) = self.context.make_not_current_in_place() {
                logger::error!("Failed to make EGL context not current during cleanup: {e}");
            } else {
                logger::info!("Successfully made EGL context not current during cleanup");
            }
        }
    }
}

fn create_wayland_display_handle(display_id: &ObjectId) -> Result<RawDisplayHandle> {
    let display = NonNull::new(display_id.as_ptr().cast::<c_void>()).ok_or_else(|| {
        LayerShikaError::InvalidInput {
            message: "Failed to create NonNull pointer for display".into(),
        }
    })?;
    let handle = WaylandDisplayHandle::new(display);
    Ok(RawDisplayHandle::Wayland(handle))
}

fn select_config(
    glutin_display: &Display,
    config_template: ConfigTemplateBuilder,
) -> Result<Config> {
    let mut configs = unsafe { glutin_display.find_configs(config_template.build()) }
        .map_err(|e| EGLError::ConfigSelection { source: e.into() })?;
    configs
        .next()
        .ok_or_else(|| EGLError::NoCompatibleConfig.into())
}

fn create_context(
    glutin_display: &Display,
    config: &Config,
    context_attributes: ContextAttributesBuilder,
) -> Result<NotCurrentContext> {
    unsafe { glutin_display.create_context(config, &context_attributes.build(None)) }
        .map_err(|e| EGLError::ContextCreation { source: e.into() }.into())
}

fn create_surface_handle(surface_id: &ObjectId) -> Result<RawWindowHandle> {
    let surface = NonNull::new(surface_id.as_ptr().cast::<c_void>()).ok_or_else(|| {
        LayerShikaError::InvalidInput {
            message: "Failed to create NonNull pointer for surface".into(),
        }
    })?;
    let handle = WaylandWindowHandle::new(surface);
    Ok(RawWindowHandle::Wayland(handle))
}

fn create_surface(
    glutin_display: &Display,
    config: &Config,
    surface_handle: RawWindowHandle,
    size: PhysicalSize,
) -> Result<Surface<WindowSurface>> {
    let width = NonZeroU32::new(size.width).ok_or_else(|| LayerShikaError::InvalidInput {
        message: "Width cannot be zero".into(),
    })?;

    let height = NonZeroU32::new(size.height).ok_or_else(|| LayerShikaError::InvalidInput {
        message: "Height cannot be zero".into(),
    })?;

    let attrs =
        SurfaceAttributesBuilder::<WindowSurface>::new().build(surface_handle, width, height);

    unsafe { glutin_display.create_window_surface(config, &attrs) }
        .map_err(|e| EGLError::SurfaceCreation { source: e.into() }.into())
}

unsafe impl OpenGLInterface for EGLContext {
    fn ensure_current(&self) -> StdResult<(), Box<dyn Error + Send + Sync>> {
        self.ensure_current()
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    fn swap_buffers(&self) -> StdResult<(), Box<dyn Error + Send + Sync>> {
        self.surface
            .swap_buffers(&self.context)
            .map_err(|e| EGLError::SwapBuffers { source: e.into() }.into())
    }

    fn resize(
        &self,
        width: NonZeroU32,
        height: NonZeroU32,
    ) -> StdResult<(), Box<dyn Error + Send + Sync>> {
        self.ensure_current()?;
        self.surface.resize(&self.context, width, height);
        Ok(())
    }

    fn get_proc_address(&self, name: &CStr) -> *const ffi::c_void {
        self.context.display().get_proc_address(name)
    }
}
