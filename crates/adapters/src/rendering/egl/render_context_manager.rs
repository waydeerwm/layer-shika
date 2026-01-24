use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::Rc;

use glutin::api::egl::config::Config;
use glutin::api::egl::context::PossiblyCurrentContext;
use glutin::api::egl::display::Display;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::ContextAttributesBuilder;
use glutin::prelude::*;
use raw_window_handle::{RawDisplayHandle, WaylandDisplayHandle};
use wayland_client::backend::ObjectId;

use crate::errors::{EGLError, LayerShikaError, Result};
use crate::logger;

pub struct RenderContextManager {
    display: Display,
    config: Config,
    root_context: PossiblyCurrentContext,
}

impl RenderContextManager {
    pub fn new(display_id: &ObjectId) -> Result<Rc<Self>> {
        logger::info!("Initializing RenderContextManager with independent root context");

        let display_handle = create_wayland_display_handle(display_id)?;
        let display = unsafe { Display::new(display_handle) }
            .map_err(|e| EGLError::DisplayCreation { source: e.into() })?;

        let config_template = ConfigTemplateBuilder::default();
        let config = select_config(&display, config_template)?;

        let root_context = Self::create_root_context(&display, &config)?;

        logger::info!("RenderContextManager initialized successfully");

        Ok(Rc::new(Self {
            display,
            config,
            root_context,
        }))
    }

    fn create_root_context(display: &Display, config: &Config) -> Result<PossiblyCurrentContext> {
        if let Ok(context) = Self::try_create_surfaceless_context(display, config) {
            logger::info!("Created surfaceless root EGL context");
            return Ok(context);
        }

        logger::warn!(
            "Surfaceless context not available, using workaround with make_current_surfaceless anyway"
        );
        Self::create_surfaceless_fallback(display, config)
    }

    fn try_create_surfaceless_context(
        display: &Display,
        config: &Config,
    ) -> Result<PossiblyCurrentContext> {
        let context_attributes = ContextAttributesBuilder::default();
        let not_current =
            unsafe { display.create_context(config, &context_attributes.build(None)) }
                .map_err(|e| EGLError::ContextCreation { source: e.into() })?;

        not_current
            .make_current_surfaceless()
            .map_err(|e| EGLError::MakeCurrent { source: e.into() }.into())
    }

    fn create_surfaceless_fallback(
        display: &Display,
        config: &Config,
    ) -> Result<PossiblyCurrentContext> {
        let context_attributes = ContextAttributesBuilder::default();
        let not_current =
            unsafe { display.create_context(config, &context_attributes.build(None)) }
                .map_err(|e| EGLError::ContextCreation { source: e.into() })?;

        not_current
            .make_current_surfaceless()
            .map_err(|e| EGLError::MakeCurrent { source: e.into() }.into())
    }

    pub fn display(&self) -> &Display {
        &self.display
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn root_context(&self) -> &PossiblyCurrentContext {
        &self.root_context
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
