use super::renderable_window::{RenderState, RenderableWindow};
use crate::{
    errors::{RenderingError, Result},
    logger,
};
use core::ops::Deref;
use slint::{
    PhysicalSize, Window, WindowSize,
    platform::{Renderer, WindowAdapter, WindowEvent, femtovg_renderer::FemtoVGRenderer},
};
use std::cell::Cell;
use std::rc::{Rc, Weak};

pub struct FemtoVGWindow {
    window: Window,
    renderer: FemtoVGRenderer,
    render_state: Cell<RenderState>,
    size: Cell<PhysicalSize>,
    scale_factor: Cell<f32>,
}

impl FemtoVGWindow {
    #[must_use]
    pub fn new(renderer: FemtoVGRenderer) -> Rc<Self> {
        Rc::new_cyclic(|weak_self| {
            let window = Window::new(Weak::clone(weak_self) as Weak<dyn WindowAdapter>);
            Self {
                window,
                renderer,
                render_state: Cell::new(RenderState::Clean),
                size: Cell::new(PhysicalSize::default()),
                scale_factor: Cell::new(1.),
            }
        })
    }
}

impl RenderableWindow for FemtoVGWindow {
    fn render_frame_if_dirty(&self) -> Result<()> {
        if matches!(
            self.render_state.replace(RenderState::Clean),
            RenderState::Dirty
        ) {
            self.renderer
                .render()
                .map_err(|e| RenderingError::Operation {
                    message: format!("Error rendering frame: {e}"),
                })?;
        }
        Ok(())
    }

    fn set_scale_factor(&self, scale_factor: f32) {
        logger::info!("Setting scale factor to {scale_factor}");
        self.scale_factor.set(scale_factor);
        self.window()
            .dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor });
    }

    fn scale_factor(&self) -> f32 {
        self.scale_factor.get()
    }

    fn render_state(&self) -> &Cell<RenderState> {
        &self.render_state
    }

    fn size_cell(&self) -> &Cell<PhysicalSize> {
        &self.size
    }
}

impl WindowAdapter for FemtoVGWindow {
    fn window(&self) -> &Window {
        &self.window
    }

    fn renderer(&self) -> &dyn Renderer {
        &self.renderer
    }

    fn size(&self) -> PhysicalSize {
        self.size_impl()
    }

    fn set_size(&self, size: WindowSize) {
        self.set_size_impl(size);
    }

    fn request_redraw(&self) {
        RenderableWindow::request_redraw(self);
    }
}

impl Deref for FemtoVGWindow {
    type Target = Window;
    fn deref(&self) -> &Self::Target {
        &self.window
    }
}
