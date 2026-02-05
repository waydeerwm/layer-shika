use std::rc::Rc;

use slint::platform::WindowAdapter;
use slint::platform::femtovg_renderer::FemtoVGRenderer;
use slint::{LogicalPosition, LogicalSize, WindowPosition, WindowSize};
use wayland_client::backend::ObjectId;

use crate::errors::{LayerShikaError, Result};
use crate::rendering::femtovg::main_window::FemtoVGWindow;
use crate::rendering::femtovg::renderable_window::RenderableWindow;
use crate::wayland::session_lock::lock_context::SessionLockContext;

pub(super) fn create_window(
    context: &SessionLockContext,
    surface_id: &ObjectId,
    scale_factor: f32,
) -> Result<Rc<FemtoVGWindow>> {
    let init_size = LogicalSize::new(1.0, 1.0);
    let render_context = context
        .render_factory()
        .create_context(surface_id, init_size.to_physical(scale_factor))?;
    let renderer = FemtoVGRenderer::new(render_context)
        .map_err(|e| LayerShikaError::FemtoVGRendererCreation { source: e })?;
    let window = FemtoVGWindow::new(renderer);
    RenderableWindow::set_scale_factor(window.as_ref(), scale_factor);
    window.set_size(WindowSize::Logical(init_size));
    window.set_position(WindowPosition::Logical(LogicalPosition::new(0., 0.)));
    Ok(window)
}
