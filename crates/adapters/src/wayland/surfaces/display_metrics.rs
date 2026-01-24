use std::cell::RefCell;
use std::rc::Rc;

use layer_shika_domain::dimensions::{
    LogicalSize as DomainLogicalSize, PhysicalSize as DomainPhysicalSize,
    ScaleFactor as DomainScaleFactor,
};
use layer_shika_domain::errors::Result as DomainResult;
use layer_shika_domain::surface_dimensions::SurfaceDimensions;
use slint::PhysicalSize;

use crate::logger;

pub struct DisplayMetrics {
    surface: SurfaceDimensions,
    output_size: PhysicalSize,
    has_fractional_scale: bool,
}

impl DisplayMetrics {
    #[must_use]
    pub fn new(scale_factor: f32, has_fractional_scale: bool) -> Self {
        let scale = DomainScaleFactor::from_raw(scale_factor);
        let logical = DomainLogicalSize::from_raw(1.0, 1.0);
        let surface = SurfaceDimensions::from_logical(logical, scale);

        Self {
            surface,
            output_size: PhysicalSize::new(0, 0),
            has_fractional_scale,
        }
    }

    #[must_use]
    pub fn with_output_size(mut self, output_size: PhysicalSize) -> Self {
        self.output_size = output_size;
        self.recalculate_surface_size();
        self
    }

    #[must_use]
    pub fn scale_factor(&self) -> f32 {
        self.surface.scale_factor().value()
    }

    #[must_use]
    pub fn scale_factor_typed(&self) -> DomainScaleFactor {
        self.surface.scale_factor()
    }

    #[must_use]
    pub const fn output_size(&self) -> PhysicalSize {
        self.output_size
    }

    #[must_use]
    pub fn surface_size(&self) -> PhysicalSize {
        PhysicalSize::new(
            self.surface.physical_width(),
            self.surface.physical_height(),
        )
    }

    #[must_use]
    pub const fn surface_dimensions(&self) -> &SurfaceDimensions {
        &self.surface
    }

    #[must_use]
    pub const fn has_fractional_scale(&self) -> bool {
        self.has_fractional_scale
    }

    pub fn calculate_dimensions(
        &self,
        logical_width: u32,
        logical_height: u32,
    ) -> DomainResult<SurfaceDimensions> {
        SurfaceDimensions::calculate(logical_width, logical_height, self.scale_factor())
    }

    pub fn scale_factor_from_120ths(scale_120ths: u32) -> f32 {
        DomainScaleFactor::from_120ths(scale_120ths).value()
    }

    #[allow(clippy::cast_precision_loss)]
    pub fn update_scale_factor(&mut self, scale_120ths: u32) -> f32 {
        let new_scale = DomainScaleFactor::from_120ths(scale_120ths);
        let new_scale_factor = new_scale.value();
        let old_scale_factor = self.scale_factor();

        if (old_scale_factor - new_scale_factor).abs() > f32::EPSILON {
            logger::info!(
                "DisplayMetrics: Updating scale factor from {} to {} ({}x)",
                old_scale_factor,
                new_scale_factor,
                scale_120ths
            );
            self.surface.update_scale_factor(new_scale);
            self.recalculate_surface_size();
        }

        new_scale_factor
    }

    pub fn update_output_size(&mut self, output_size: PhysicalSize) {
        if self.output_size != output_size {
            logger::info!(
                "DisplayMetrics: Updating output size from {:?} to {:?}",
                self.output_size,
                output_size
            );
            self.output_size = output_size;
            self.recalculate_surface_size();
        }
    }

    pub fn update_surface_size(&mut self, surface_size: PhysicalSize) {
        let physical = DomainPhysicalSize::from_raw(surface_size.width, surface_size.height);
        self.surface = SurfaceDimensions::from_physical(physical, self.surface.scale_factor());
    }

    fn recalculate_surface_size(&mut self) {
        if self.output_size.width > 0 && self.output_size.height > 0 && self.scale_factor() > 0.0 {
            let physical =
                DomainPhysicalSize::from_raw(self.output_size.width, self.output_size.height);
            self.surface = SurfaceDimensions::from_physical(physical, self.surface.scale_factor());
        }
    }
}

pub type SharedDisplayMetrics = Rc<RefCell<DisplayMetrics>>;
