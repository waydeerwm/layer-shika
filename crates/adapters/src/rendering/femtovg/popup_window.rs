use super::renderable_window::{RenderState, RenderableWindow};
use crate::errors::{RenderingError, Result};
use crate::logger;
use crate::wayland::surfaces::popup_manager::OnCloseCallback;
use core::ops::Deref;
use layer_shika_domain::dimensions::LogicalSize;
use layer_shika_domain::value_objects::handle::PopupHandle;
use slint::{
    PhysicalSize, Window, WindowSize,
    platform::{Renderer, WindowAdapter, WindowEvent, femtovg_renderer::FemtoVGRenderer},
};
use slint_interpreter::ComponentInstance;
use std::cell::{Cell, OnceCell, RefCell};
use std::rc::{Rc, Weak};

/// Represents the rendering lifecycle state of a popup window
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PopupRenderState {
    /// Awaiting Wayland configure event before rendering can begin
    Unconfigured,
    /// Wayland is recalculating geometry; rendering is paused
    Repositioning,
    /// Ready to render, no pending changes
    ReadyClean,
    /// Ready to render, frame is dirty and needs redraw
    ReadyDirty,
    /// Needs an additional layout pass after the next render
    NeedsRelayout,
}

pub struct PopupWindow {
    window: Window,
    renderer: FemtoVGRenderer,
    render_state: Cell<RenderState>,
    size: Cell<PhysicalSize>,
    scale_factor: Cell<f32>,
    popup_handle: Cell<Option<PopupHandle>>,
    on_close: OnceCell<OnCloseCallback>,
    popup_render_state: Cell<PopupRenderState>,
    component_instance: RefCell<Option<ComponentInstance>>,
    logical_size: Cell<LogicalSize>,
}

impl PopupWindow {
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
                popup_handle: Cell::new(None),
                on_close: OnceCell::new(),
                popup_render_state: Cell::new(PopupRenderState::Unconfigured),
                component_instance: RefCell::new(None),
                logical_size: Cell::new(LogicalSize::default()),
            }
        })
    }

    #[must_use]
    pub fn new_with_callback(renderer: FemtoVGRenderer, on_close: OnCloseCallback) -> Rc<Self> {
        let window = Self::new(renderer);
        window.on_close.set(on_close).ok();
        window
    }

    pub fn set_popup_id(&self, handle: PopupHandle) {
        self.popup_handle.set(Some(handle));
    }

    pub(crate) fn cleanup_resources(&self) {
        logger::info!("Cleaning up popup window resources to break reference cycles");

        if let Err(e) = self.window.hide() {
            logger::info!("Failed to hide popup window: {e}");
        }

        if let Some(component) = self.component_instance.borrow_mut().take() {
            logger::info!("Dropping ComponentInstance to break reference cycle");
            drop(component);
        }

        logger::info!("Popup window resource cleanup complete");
    }

    pub fn close_popup(&self) {
        logger::info!("Closing popup window - cleaning up resources");

        self.cleanup_resources();

        if let Some(handle) = self.popup_handle.get() {
            logger::info!("Destroying popup with handle {:?}", handle);
            if let Some(on_close) = self.on_close.get() {
                on_close(handle);
            }
        }

        self.popup_handle.set(None);

        logger::info!("Popup window cleanup complete");
    }

    pub fn popup_key(&self) -> Option<usize> {
        self.popup_handle.get().map(PopupHandle::key)
    }

    pub fn mark_configured(&self) {
        logger::info!("Popup window marked as configured");

        if matches!(
            self.popup_render_state.get(),
            PopupRenderState::Unconfigured
        ) {
            logger::info!("Transitioning from Unconfigured to ReadyDirty state");
            self.popup_render_state.set(PopupRenderState::ReadyDirty);
        } else {
            logger::info!(
                "Preserving current render state to avoid overwriting: {:?}",
                self.popup_render_state.get()
            );
        }
    }

    pub fn is_configured(&self) -> bool {
        !matches!(
            self.popup_render_state.get(),
            PopupRenderState::Unconfigured
        )
    }

    pub fn set_component_instance(&self, instance: ComponentInstance) {
        logger::info!("Setting component instance for popup window");
        let mut comp = self.component_instance.borrow_mut();
        if comp.is_some() {
            logger::info!("Component instance already set for popup window - replacing");
        }
        *comp = Some(instance);

        self.window()
            .dispatch_event(WindowEvent::WindowActiveChanged(true));
    }

    pub fn request_resize(&self, width: f32, height: f32) -> bool {
        let new_size = LogicalSize::from_raw(width, height);
        let current_size = self.logical_size.get();

        if current_size == new_size {
            logger::info!(
                "Popup resize skipped - size unchanged: {}x{}",
                width,
                height
            );
            return false;
        }

        logger::info!(
            "Requesting popup resize from {}x{} to {}x{}",
            current_size.width(),
            current_size.height(),
            width,
            height
        );
        self.logical_size.set(new_size);
        self.set_size(WindowSize::Logical(slint::LogicalSize::new(width, height)));
        RenderableWindow::request_redraw(self);
        true
    }

    pub fn begin_repositioning(&self) {
        self.popup_render_state.set(PopupRenderState::Repositioning);
    }

    pub fn end_repositioning(&self) {
        self.popup_render_state.set(PopupRenderState::NeedsRelayout);
    }
}

impl RenderableWindow for PopupWindow {
    fn render_frame_if_dirty(&self) -> Result<()> {
        match self.popup_render_state.get() {
            PopupRenderState::Unconfigured => {
                logger::info!("Popup not yet configured, skipping render");
                return Ok(());
            }
            PopupRenderState::Repositioning => {
                logger::info!("Popup repositioning in progress, skipping render");
                return Ok(());
            }
            PopupRenderState::ReadyClean => {
                // Nothing to render
                return Ok(());
            }
            PopupRenderState::ReadyDirty | PopupRenderState::NeedsRelayout => {
                // Proceed with rendering
            }
        }

        if matches!(
            self.render_state.replace(RenderState::Clean),
            RenderState::Dirty
        ) {
            self.renderer
                .render()
                .map_err(|e| RenderingError::Operation {
                    message: format!("Error rendering popup frame: {e}"),
                })?;

            if matches!(
                self.popup_render_state.get(),
                PopupRenderState::NeedsRelayout
            ) {
                logger::info!("Popup needs relayout, requesting additional render");
                self.popup_render_state.set(PopupRenderState::ReadyDirty);
                RenderableWindow::request_redraw(self);
            } else {
                self.popup_render_state.set(PopupRenderState::ReadyClean);
            }
        }
        Ok(())
    }

    fn set_scale_factor(&self, scale_factor: f32) {
        logger::info!("Setting popup scale factor to {scale_factor}");
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

impl WindowAdapter for PopupWindow {
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
        if matches!(self.popup_render_state.get(), PopupRenderState::ReadyClean) {
            self.popup_render_state.set(PopupRenderState::ReadyDirty);
        }
        RenderableWindow::request_redraw(self);
    }
}

impl Deref for PopupWindow {
    type Target = Window;
    fn deref(&self) -> &Self::Target {
        &self.window
    }
}

impl Drop for PopupWindow {
    fn drop(&mut self) {
        logger::info!("PopupWindow being dropped - cleaning up resources");

        if let Some(component) = self.component_instance.borrow_mut().take() {
            logger::info!("Dropping any remaining ComponentInstance in PopupWindow::drop");
            drop(component);
        }

        logger::info!("PopupWindow drop complete");
    }
}
