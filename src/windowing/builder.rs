use anyhow::Result;
use slint_interpreter::ComponentDefinition;
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{self},
    zwlr_layer_surface_v1::{Anchor, KeyboardInteractivity},
};

use super::{config::WindowConfig, WindowingSystem};

pub struct WindowingSystemBuilder {
    config: WindowConfig,
}

impl Default for WindowingSystemBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowingSystemBuilder {
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: WindowConfig::default(),
        }
    }

    #[must_use]
    pub const fn with_height(mut self, height: u32) -> Self {
        self.config.height = height;
        self
    }

    #[must_use]
    pub const fn with_layer(mut self, layer: zwlr_layer_shell_v1::Layer) -> Self {
        self.config.layer = layer;
        self
    }

    #[must_use]
    pub const fn with_margin(mut self, top: i32, right: i32, bottom: i32, left: i32) -> Self {
        self.config.margin = (top, right, bottom, left);
        self
    }

    #[must_use]
    pub const fn with_anchor(mut self, anchor: Anchor) -> Self {
        self.config.anchor = anchor;
        self
    }

    #[must_use]
    pub const fn with_keyboard_interactivity(
        mut self,
        interactivity: KeyboardInteractivity,
    ) -> Self {
        self.config.keyboard_interactivity = interactivity;
        self
    }

    #[must_use]
    pub const fn with_exclusive_zone(mut self, zone: i32) -> Self {
        self.config.exclusive_zone = zone;
        self
    }

    #[must_use]
    pub fn with_namespace(mut self, namespace: String) -> Self {
        self.config.namespace = namespace;
        self
    }

    #[must_use]
    pub const fn with_scale_factor(mut self, scale_factor: f32) -> Self {
        self.config.scale_factor = scale_factor;
        self
    }

    #[must_use]
    pub fn with_component_definition(mut self, component: ComponentDefinition) -> Self {
        self.config.component_definition = Some(component);
        self
    }

    pub fn build(self) -> Result<WindowingSystem> {
        match self.config.component_definition {
            Some(_) => WindowingSystem::new(&self.config),
            None => Err(anyhow::anyhow!("Slint component not set")),
        }
    }
}
