use std::rc::Rc;
use slint::PhysicalSize;
use slint_interpreter::ComponentDefinition;
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1;
use wayland_client::protocol::{wl_pointer::WlPointer, wl_surface::WlSurface};
use crate::{errors::LayerShikaError, rendering::{femtovg_window::FemtoVGWindow, slint_platform::CustomSlintPlatform}};

use super::WindowState;

pub struct WindowStateBuilder {
    pub component_definition: Option<ComponentDefinition>,
    pub surface: Option<Rc<WlSurface>>,
    pub layer_surface: Option<Rc<ZwlrLayerSurfaceV1>>,
    pub size: Option<PhysicalSize>,
    pub output_size: Option<PhysicalSize>,
    pub pointer: Option<Rc<WlPointer>>,
    pub window: Option<Rc<FemtoVGWindow>>,
    pub scale_factor: f32,
    pub height: u32,
    pub exclusive_zone: i32,
}

impl WindowStateBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_surface(mut self, surface: Rc<WlSurface>) -> Self {
        self.surface = Some(surface);
        self
    }

    #[must_use]
    pub fn with_layer_surface(mut self, layer_surface: Rc<ZwlrLayerSurfaceV1>) -> Self {
        self.layer_surface = Some(layer_surface);
        self
    }

    #[must_use]
    pub const fn with_size(mut self, size: PhysicalSize) -> Self {
        self.size = Some(size);
        self
    }

    #[must_use]
    pub const fn with_output_size(mut self, output_size: PhysicalSize) -> Self {
        self.output_size = Some(output_size);
        self
    }

    #[must_use]
    pub fn with_pointer(mut self, pointer: Rc<WlPointer>) -> Self {
        self.pointer = Some(pointer);
        self
    }

    #[must_use]
    pub fn with_window(mut self, window: Rc<FemtoVGWindow>) -> Self {
        self.window = Some(window);
        self
    }

    #[must_use]
    pub const fn with_scale_factor(mut self, scale_factor: f32) -> Self {
        self.scale_factor = scale_factor;
        self
    }

    #[must_use]
    pub const fn with_height(mut self, height: u32) -> Self {
        self.height = height;
        self
    }

    #[must_use]
    pub const fn with_exclusive_zone(mut self, exclusive_zone: i32) -> Self {
        self.exclusive_zone = exclusive_zone;
        self
    }

    #[must_use]
    pub fn with_component_definition(mut self, component_definition: ComponentDefinition) -> Self {
        self.component_definition = Some(component_definition);
        self
    }

    pub fn build(self) -> Result<WindowState, LayerShikaError> {
        let platform = CustomSlintPlatform::new(Rc::clone(
            self.window
                .as_ref()
                .ok_or_else(|| LayerShikaError::InvalidInput("Window is required".into()))?,
        ));
        slint::platform::set_platform(Box::new(platform)).map_err(|e| {
            LayerShikaError::PlatformSetup(format!("Failed to set platform: {e:?}"))
        })?;

        WindowState::new(self)
    }
}

impl Default for WindowStateBuilder {
    fn default() -> Self {
        Self {
            component_definition: None,
            surface: None,
            layer_surface: None,
            size: None,
            output_size: None,
            pointer: None,
            window: None,
            scale_factor: 1.0,
            height: 30,
            exclusive_zone: -1,
        }
    }
}
