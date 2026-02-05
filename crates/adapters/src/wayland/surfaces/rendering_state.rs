use std::rc::Rc;
use crate::errors::Result;
use crate::rendering::femtovg::renderable_window::RenderableWindow;
use crate::wayland::surfaces::surface_renderer::{SurfaceRenderer, SurfaceRendererParams};
use slint::PhysicalSize;
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1;
use wayland_client::protocol::wl_surface::WlSurface;
use crate::wayland::managed_proxies::{ManagedWpFractionalScaleV1};

pub struct RenderingState<W: RenderableWindow> {
    renderer: SurfaceRenderer<W>,
}

impl<W: RenderableWindow> RenderingState<W> {
    #[must_use]
    pub fn new(params: SurfaceRendererParams<W>) -> Self {
        Self {
            renderer: SurfaceRenderer::new(params),
        }
    }

    pub fn render_frame_if_dirty(&self) -> Result<()> {
        self.renderer.render_frame_if_dirty()
    }

    pub fn update_size(&mut self, width: u32, height: u32, scale_factor: f32) {
        self.renderer.update_size(width, height, scale_factor);
    }

    pub const fn size(&self) -> PhysicalSize {
        self.renderer.size()
    }

    pub const fn logical_size(&self) -> PhysicalSize {
        self.renderer.logical_size()
    }

    pub const fn height(&self) -> u32 {
        self.renderer.height()
    }

    pub const fn window(&self) -> &Rc<W> {
        self.renderer.window()
    }

    pub fn layer_surface(&self) -> Rc<ZwlrLayerSurfaceV1> {
        self.renderer.layer_surface()
    }

    pub fn surface(&self) -> Rc<WlSurface> {
        self.renderer.surface()
    }

    pub fn fractional_scale(&self) -> Option<&ManagedWpFractionalScaleV1> {
        self.renderer.fractional_scale()
    }

    pub fn commit_surface(&self) {
        self.renderer.commit_surface();
    }
}
