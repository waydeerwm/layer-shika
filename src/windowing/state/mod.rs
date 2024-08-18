use std::rc::Rc;
use log::info;
use slint::{LogicalPosition, PhysicalSize};
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1;
use wayland_client::protocol::{wl_pointer::WlPointer, wl_surface::WlSurface};
use crate::rendering::femtovg_window::FemtoVGWindow;
use super::WindowConfig;

pub mod dispatches;

pub struct WindowState {
    surface: Option<Rc<WlSurface>>,
    layer_surface: Option<Rc<ZwlrLayerSurfaceV1>>,
    size: PhysicalSize,
    output_size: PhysicalSize,
    pointer: Option<Rc<WlPointer>>,
    window: Option<Rc<FemtoVGWindow>>,
    current_pointer_position: LogicalPosition,
    scale_factor: f32,
    height: u32,
    exclusive_zone: i32,
}

impl WindowState {
    pub fn new(config: &WindowConfig) -> Self {
        Self {
            surface: None,
            layer_surface: None,
            size: PhysicalSize::default(),
            output_size: PhysicalSize::default(),
            pointer: None,
            window: None,
            current_pointer_position: LogicalPosition::default(),
            scale_factor: config.scale_factor,
            height: config.height,
            exclusive_zone: config.exclusive_zone,
        }
    }

    pub fn update_size(&mut self, width: u32, height: u32) {
        let new_size = PhysicalSize::new(width, height);
        if let Some(window) = &self.window() {
            info!("Updating window size to {}x{}", width, height);
            window.set_size(slint::WindowSize::Physical(new_size));
            window.set_scale_factor(self.scale_factor);
        }

        if let Some(layer_surface) = &self.layer_surface() {
            info!("Updating layer surface size to {}x{}", width, height);
            layer_surface.set_size(width, height);
            layer_surface.set_exclusive_zone(self.exclusive_zone);
        }

        if let Some(s) = self.surface.as_ref() {
            s.commit();
        }
        self.size = new_size;
    }

    pub fn set_current_pointer_position(&mut self, physical_x: f64, physical_y: f64) {
        let scale_factor = self.scale_factor;
        let logical_position = LogicalPosition::new(
            physical_x as f32 / scale_factor,
            physical_y as f32 / scale_factor,
        );
        self.current_pointer_position = logical_position;
    }

    pub const fn size(&self) -> PhysicalSize {
        self.size
    }

    pub const fn current_pointer_position(&self) -> LogicalPosition {
        self.current_pointer_position
    }
    pub fn window(&self) -> Option<Rc<FemtoVGWindow>> {
        self.window.clone()
    }

    pub fn layer_surface(&self) -> Option<Rc<ZwlrLayerSurfaceV1>> {
        self.layer_surface.clone()
    }
    pub fn surface(&self) -> Option<Rc<WlSurface>> {
        self.surface.clone()
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub fn set_output_size(&mut self, output_size: PhysicalSize) {
        self.output_size = output_size;
    }

    pub const fn output_size(&self) -> PhysicalSize {
        self.output_size
    }

    pub fn set_window(&mut self, window: Rc<FemtoVGWindow>) {
        self.window = Some(window);
    }

    pub fn set_layer_surface(&mut self, layer_surface: Rc<ZwlrLayerSurfaceV1>) {
        self.layer_surface = Some(layer_surface);
    }

    pub fn set_surface(&mut self, surface: Rc<WlSurface>) {
        self.surface = Some(surface);
    }
    pub fn set_pointer(&mut self, pointer: Rc<WlPointer>) {
        self.pointer = Some(pointer);
    }
}
