use crate::errors::Result;
use crate::rendering::femtovg::renderable_window::{FractionalScaleConfig, RenderableWindow};
use crate::wayland::managed_proxies::{
    ManagedWlSurface, ManagedZwlrLayerSurfaceV1, ManagedWpFractionalScaleV1, ManagedWpViewport,
};
use crate::wayland::surfaces::dimensions::SurfaceDimensionsExt;
use crate::logger;
use layer_shika_domain::surface_dimensions::SurfaceDimensions;
use slint::PhysicalSize;
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScalingMode {
    FractionalWithViewport,
    FractionalOnly,
    Integer,
}

pub struct SurfaceRendererParams<W: RenderableWindow> {
    pub window: Rc<W>,
    pub surface: ManagedWlSurface,
    pub layer_surface: ManagedZwlrLayerSurfaceV1,
    pub viewport: Option<ManagedWpViewport>,
    pub fractional_scale: Option<ManagedWpFractionalScaleV1>,
    pub height: u32,
    pub size: PhysicalSize,
}

pub struct SurfaceRenderer<W: RenderableWindow> {
    window: Rc<W>,
    surface: ManagedWlSurface,
    layer_surface: ManagedZwlrLayerSurfaceV1,
    viewport: Option<ManagedWpViewport>,
    fractional_scale: Option<ManagedWpFractionalScaleV1>,
    height: u32,
    size: PhysicalSize,
    logical_size: PhysicalSize,
}

impl<W: RenderableWindow> SurfaceRenderer<W> {
    #[must_use]
    pub fn new(params: SurfaceRendererParams<W>) -> Self {
        Self {
            window: params.window,
            surface: params.surface,
            layer_surface: params.layer_surface,
            viewport: params.viewport,
            fractional_scale: params.fractional_scale,
            height: params.height,
            size: params.size,
            logical_size: PhysicalSize::default(),
        }
    }

    pub fn render_frame_if_dirty(&self) -> Result<()> {
        self.window.render_frame_if_dirty()
    }

    pub const fn window(&self) -> &Rc<W> {
        &self.window
    }

    pub fn layer_surface(&self) -> Rc<ZwlrLayerSurfaceV1> {
        Rc::clone(self.layer_surface.inner())
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub const fn size(&self) -> PhysicalSize {
        self.size
    }

    const fn determine_scaling_mode(&self) -> ScalingMode {
        if self.fractional_scale.is_some() && self.viewport.is_some() {
            ScalingMode::FractionalWithViewport
        } else if self.fractional_scale.is_some() {
            ScalingMode::FractionalOnly
        } else {
            ScalingMode::Integer
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn configure_slint_window(
        &self,
        dimensions: &SurfaceDimensions,
        mode: ScalingMode,
        scale_factor: f32,
    ) {
        match mode {
            ScalingMode::FractionalWithViewport => {
                let config = FractionalScaleConfig::new(
                    dimensions.logical_width() as f32,
                    dimensions.logical_height() as f32,
                    scale_factor,
                );
                logger::info!(
                    "FractionalWithViewport: render scale {} (from {}), physical {}x{}",
                    config.render_scale,
                    scale_factor,
                    config.render_physical_size.width,
                    config.render_physical_size.height
                );
                config.apply_to(self.window.as_ref());
            }
            ScalingMode::FractionalOnly => {
                self.window
                    .set_scale_factor(dimensions.buffer_scale() as f32);
                self.window
                    .set_size(slint::WindowSize::Logical(slint::LogicalSize::new(
                        dimensions.logical_width() as f32,
                        dimensions.logical_height() as f32,
                    )));
            }
            ScalingMode::Integer => {
                self.window.set_scale_factor(scale_factor);
                self.window.set_size(slint::WindowSize::Physical(
                    dimensions.to_slint_physical_size(),
                ));
            }
        }
    }

    #[allow(clippy::cast_possible_wrap)]
    fn configure_wayland_surface(&self, dimensions: &SurfaceDimensions, mode: ScalingMode) {
        match mode {
            ScalingMode::FractionalWithViewport => {
                self.surface.set_buffer_scale(1);
                if let Some(viewport) = &self.viewport {
                    viewport.set_destination(
                        dimensions.logical_width() as i32,
                        dimensions.logical_height() as i32,
                    );
                }
            }
            ScalingMode::FractionalOnly | ScalingMode::Integer => {
                self.surface.set_buffer_scale(dimensions.buffer_scale());
            }
        }

        self.surface.commit();
    }

    pub fn update_size(&mut self, width: u32, height: u32, scale_factor: f32) {
        if width == 0 || height == 0 {
            logger::info!("Skipping update_size with zero dimension: {width}x{height}");
            return;
        }

        let dimensions = match SurfaceDimensions::calculate(width, height, scale_factor) {
            Ok(d) => d,
            Err(e) => {
                logger::error!("Failed to calculate surface dimensions: {e}");
                return;
            }
        };

        self.apply_surface_dimensions(dimensions, scale_factor);
    }

    #[allow(clippy::cast_precision_loss)]
    pub fn apply_surface_dimensions(&mut self, dimensions: SurfaceDimensions, scale_factor: f32) {
        let scaling_mode = self.determine_scaling_mode();

        let render_physical_size = match scaling_mode {
            ScalingMode::FractionalWithViewport => {
                FractionalScaleConfig::new(
                    dimensions.logical_width() as f32,
                    dimensions.logical_height() as f32,
                    scale_factor,
                )
                .render_physical_size
            }
            _ => dimensions.to_slint_physical_size(),
        };

        logger::info!(
            "Updating window size: logical {}x{}, physical {}x{}, render_physical {}x{}, scale {}, buffer_scale {}, mode {:?}",
            dimensions.logical_width(),
            dimensions.logical_height(),
            dimensions.physical_width(),
            dimensions.physical_height(),
            render_physical_size.width,
            render_physical_size.height,
            scale_factor,
            dimensions.buffer_scale(),
            scaling_mode
        );

        self.configure_slint_window(&dimensions, scaling_mode, scale_factor);
        self.configure_wayland_surface(&dimensions, scaling_mode);

        logger::info!("Window physical size: {:?}", self.window.size());

        self.size = render_physical_size;
        self.logical_size = dimensions.to_slint_logical_size();
        RenderableWindow::request_redraw(self.window.as_ref());
    }

    pub const fn logical_size(&self) -> PhysicalSize {
        self.logical_size
    }

    pub const fn fractional_scale(&self) -> Option<&ManagedWpFractionalScaleV1> {
        self.fractional_scale.as_ref()
    }

    pub fn commit_surface(&self) {
        self.surface.commit();
    }
}
