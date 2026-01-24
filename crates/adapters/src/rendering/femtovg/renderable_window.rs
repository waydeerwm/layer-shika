use std::cell::Cell;

use slint::platform::{WindowAdapter, WindowEvent};
use slint::{PhysicalSize, WindowSize};

use crate::errors::Result;

pub enum RenderState {
    Clean,
    Dirty,
}

#[derive(Debug, Clone, Copy)]
pub struct FractionalScaleConfig {
    pub render_scale: f32,
    pub render_physical_size: PhysicalSize,
    pub logical_width: f32,
    pub logical_height: f32,
}

impl FractionalScaleConfig {
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    #[must_use]
    pub fn new(logical_width: f32, logical_height: f32, scale_factor: f32) -> Self {
        let render_scale = Self::render_scale(scale_factor);
        Self {
            render_scale,
            render_physical_size: PhysicalSize::new(
                (logical_width * render_scale) as u32,
                (logical_height * render_scale) as u32,
            ),
            logical_width,
            logical_height,
        }
    }

    #[must_use]
    pub fn render_scale(scale_factor: f32) -> f32 {
        scale_factor.ceil()
    }

    pub fn apply_to<W: RenderableWindow + ?Sized>(&self, window: &W) {
        window.set_scale_factor(self.render_scale);
        window.set_size_with_exact_logical(
            self.render_physical_size,
            self.logical_width,
            self.logical_height,
        );
    }
}

pub trait RenderableWindow: WindowAdapter {
    fn render_frame_if_dirty(&self) -> Result<()>;
    fn set_scale_factor(&self, scale_factor: f32);
    fn scale_factor(&self) -> f32;
    fn render_state(&self) -> &Cell<RenderState>;
    fn size_cell(&self) -> &Cell<PhysicalSize>;

    fn request_redraw(&self) {
        self.render_state().set(RenderState::Dirty);
    }

    fn size_impl(&self) -> PhysicalSize {
        self.size_cell().get()
    }

    fn set_size_impl(&self, size: WindowSize) {
        self.size_cell().set(size.to_physical(self.scale_factor()));
        self.window().dispatch_event(WindowEvent::Resized {
            size: size.to_logical(self.scale_factor()),
        });
    }

    fn set_size_with_exact_logical(
        &self,
        physical: slint::PhysicalSize,
        logical_width: f32,
        logical_height: f32,
    ) {
        self.size_cell().set(physical);
        self.window().dispatch_event(WindowEvent::Resized {
            size: slint::LogicalSize::new(logical_width, logical_height),
        });
    }
}
