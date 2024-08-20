use std::rc::Rc;
use builder::WindowStateBuilder;
use log::info;
use slint::{LogicalPosition, PhysicalSize, ComponentHandle};
use slint_interpreter::ComponentInstance;
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1;
use wayland_client::protocol::wl_surface::WlSurface;
use crate::rendering::femtovg_window::FemtoVGWindow;
use anyhow::{Context, Result};

pub mod builder;
pub mod dispatches;

pub struct WindowState {
    component_instance: ComponentInstance,
    surface: Rc<WlSurface>,
    layer_surface: Rc<ZwlrLayerSurfaceV1>,
    size: PhysicalSize,
    output_size: PhysicalSize,
    window: Rc<FemtoVGWindow>,
    current_pointer_position: LogicalPosition,
    scale_factor: f32,
    height: u32,
    exclusive_zone: i32,
}

impl WindowState {
    pub fn new(builder: WindowStateBuilder) -> Result<Self> {
        let component_definition = builder
            .component_definition
            .context("Component definition is required")?;
        let component_instance = component_definition
            .create()
            .context("Failed to create component instance")?;
        component_instance
            .show()
            .context("Failed to show component")?;
        Ok(Self {
            component_instance,
            surface: builder.surface.context("Surface is required")?,
            layer_surface: builder.layer_surface.context("Layer surface is required")?,
            size: builder.size.unwrap_or_default(),
            output_size: builder.output_size.unwrap_or_default(),
            window: builder.window.context("Window is required")?,
            current_pointer_position: LogicalPosition::default(),
            scale_factor: builder.scale_factor,
            height: builder.height,
            exclusive_zone: builder.exclusive_zone,
        })
    }

    pub fn update_size(&mut self, width: u32, height: u32) {
        let new_size = PhysicalSize::new(width, height);
        info!("Updating window size to {}x{}", width, height);
        self.window.set_size(slint::WindowSize::Physical(new_size));
        self.window.set_scale_factor(self.scale_factor);

        info!("Updating layer surface size to {}x{}", width, height);
        self.layer_surface.set_size(width, height);
        self.layer_surface.set_exclusive_zone(self.exclusive_zone);

        self.surface.commit();
        self.size = new_size;
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn set_current_pointer_position(&mut self, physical_x: f64, physical_y: f64) {
        let scale_factor = self.scale_factor;
        let logical_position = LogicalPosition::new(
            physical_x as f32 / scale_factor,
            physical_y as f32 / scale_factor,
        );
        self.current_pointer_position = logical_position;
    }

    pub const fn size(&self) -> &PhysicalSize {
        &self.size
    }

    pub const fn current_pointer_position(&self) -> &LogicalPosition {
        &self.current_pointer_position
    }

    pub fn window(&self) -> Rc<FemtoVGWindow> {
        Rc::clone(&self.window)
    }

    pub fn layer_surface(&self) -> Rc<ZwlrLayerSurfaceV1> {
        Rc::clone(&self.layer_surface)
    }

    pub fn surface(&self) -> Rc<WlSurface> {
        Rc::clone(&self.surface)
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub fn set_output_size(&mut self, output_size: PhysicalSize) {
        self.output_size = output_size;
    }

    pub const fn output_size(&self) -> &PhysicalSize {
        &self.output_size
    }

    pub const fn component_instance(&self) -> &ComponentInstance {
        &self.component_instance
    }
}
