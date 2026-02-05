use std::ffi::c_void;
use std::num::NonZeroU32;
use std::ptr::NonNull;
use std::rc::Rc;

use glutin::api::egl::config::Config;
use glutin::api::egl::display::Display;
use glutin::api::egl::surface::Surface;
use glutin::context::ContextAttributesBuilder;
use glutin::prelude::*;
use glutin::surface::{SurfaceAttributesBuilder, WindowSurface};
use raw_window_handle::{RawWindowHandle, WaylandWindowHandle};
use slint::PhysicalSize;
use wayland_client::backend::ObjectId;

use super::context::EGLContext;
use super::render_context_manager::RenderContextManager;
use crate::errors::{EGLError, LayerShikaError, Result};
use crate::logger;

pub struct RenderContextFactory {
    manager: Rc<RenderContextManager>,
}

impl RenderContextFactory {
    #[must_use]
    pub fn new(manager: Rc<RenderContextManager>) -> Rc<Self> {
        Rc::new(Self { manager })
    }

    pub fn create_context(&self, surface_id: &ObjectId, size: PhysicalSize) -> Result<EGLContext> {
        logger::info!("Creating shared EGL context from root context manager");

        let context_attributes =
            ContextAttributesBuilder::default().with_sharing(self.manager.root_context());

        let not_current = unsafe {
            self.manager
                .display()
                .create_context(self.manager.config(), &context_attributes.build(None))
        }
        .map_err(|e| EGLError::ContextCreation { source: e.into() })?;

        let surface_handle = create_surface_handle(surface_id)?;
        let surface = create_surface(
            self.manager.display(),
            self.manager.config(),
            surface_handle,
            size,
        )?;

        let context = not_current
            .make_current(&surface)
            .map_err(|e| EGLError::MakeCurrent { source: e.into() })?;

        logger::info!("Shared EGL context created successfully from root manager");

        Ok(EGLContext::from_raw(surface, context))
    }
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
