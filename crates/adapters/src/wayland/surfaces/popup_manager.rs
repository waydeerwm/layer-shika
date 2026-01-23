use crate::errors::{LayerShikaError, Result};
use crate::rendering::egl::context_factory::RenderContextFactory;
use crate::rendering::femtovg::popup_window::PopupWindow;
use crate::rendering::femtovg::renderable_window::{FractionalScaleConfig, RenderableWindow};
use crate::wayland::surfaces::display_metrics::{DisplayMetrics, SharedDisplayMetrics};
use crate::logger;
use layer_shika_domain::dimensions::LogicalSize as DomainLogicalSize;
use layer_shika_domain::surface_dimensions::SurfaceDimensions;
use layer_shika_domain::value_objects::handle::PopupHandle;
use layer_shika_domain::value_objects::popup_behavior::ConstraintAdjustment;
use layer_shika_domain::value_objects::popup_config::PopupConfig;
use layer_shika_domain::value_objects::popup_position::PopupPosition;
use slint::{platform::femtovg_renderer::FemtoVGRenderer, PhysicalSize};
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use wayland_client::{
    backend::ObjectId,
    protocol::{wl_compositor::WlCompositor, wl_seat::WlSeat, wl_surface::WlSurface},
    Connection, Proxy, QueueHandle,
};
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::WpFractionalScaleV1;
use wayland_protocols::wp::viewporter::client::wp_viewporter::WpViewporter;
use wayland_protocols::xdg::shell::client::xdg_wm_base::XdgWmBase;

use super::app_state::AppState;
use super::popup_surface::PopupSurface;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveWindow {
    Main,
    Popup(PopupHandle),
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PopupId(usize);

impl PopupId {
    #[must_use]
    const fn key(self) -> usize {
        self.0
    }

    #[must_use]
    const fn from_handle(handle: PopupHandle) -> Self {
        Self(handle.key())
    }

    #[must_use]
    const fn to_handle(self) -> PopupHandle {
        PopupHandle::from_raw(self.0)
    }
}

pub type OnCloseCallback = Box<dyn Fn(PopupHandle)>;

#[derive(Debug, Clone)]
pub struct CreatePopupParams {
    pub last_pointer_serial: u32,
    pub width: f32,
    pub height: f32,
    pub position: PopupPosition,
    pub constraint_adjustment: ConstraintAdjustment,
    pub grab: bool,
}

#[derive(Clone)]
pub struct PopupContext {
    compositor: WlCompositor,
    xdg_wm_base: Option<XdgWmBase>,
    seat: WlSeat,
    fractional_scale_manager: Option<WpFractionalScaleManagerV1>,
    viewporter: Option<WpViewporter>,
    render_factory: Rc<RenderContextFactory>,
}

impl PopupContext {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        compositor: WlCompositor,
        xdg_wm_base: Option<XdgWmBase>,
        seat: WlSeat,
        fractional_scale_manager: Option<WpFractionalScaleManagerV1>,
        viewporter: Option<WpViewporter>,
        _connection: Rc<Connection>,
        render_factory: Rc<RenderContextFactory>,
    ) -> Self {
        Self {
            compositor,
            xdg_wm_base,
            seat,
            fractional_scale_manager,
            viewporter,
            render_factory,
        }
    }
}

struct ActivePopup {
    surface: PopupSurface,
    window: Rc<PopupWindow>,
}

impl Drop for ActivePopup {
    fn drop(&mut self) {
        logger::info!("ActivePopup being dropped - cleaning up resources");
        self.window.cleanup_resources();
        self.surface.destroy();
    }
}

struct PendingPopup {
    id: PopupId,
    config: PopupConfig,
    width: f32,
    height: f32,
}

struct PopupManagerState {
    popups: HashMap<PopupId, ActivePopup>,
    display_metrics: SharedDisplayMetrics,
    pending_popups: VecDeque<PendingPopup>,
}

impl PopupManagerState {
    fn new(display_metrics: SharedDisplayMetrics) -> Self {
        Self {
            popups: HashMap::new(),
            display_metrics,
            pending_popups: VecDeque::new(),
        }
    }
}

pub struct PopupManager {
    context: PopupContext,
    state: RefCell<PopupManagerState>,
    scale_factor: Cell<f32>,
}

impl PopupManager {
    #[must_use]
    pub fn new(context: PopupContext, display_metrics: SharedDisplayMetrics) -> Self {
        let scale_factor = display_metrics.borrow().scale_factor();
        Self {
            context,
            state: RefCell::new(PopupManagerState::new(display_metrics)),
            scale_factor: Cell::new(scale_factor),
        }
    }

    pub fn request_popup(
        &self,
        handle: PopupHandle,
        config: PopupConfig,
        width: f32,
        height: f32,
    ) -> PopupHandle {
        let mut state = self.state.borrow_mut();

        let id = PopupId::from_handle(handle);

        state.pending_popups.push_back(PendingPopup {
            id,
            config,
            width,
            height,
        });

        handle
    }

    #[must_use]
    pub(crate) fn take_pending_popup_params(&self) -> Option<(PopupId, PopupConfig, f32, f32)> {
        self.state
            .borrow_mut()
            .pending_popups
            .pop_front()
            .map(|p| (p.id, p.config, p.width, p.height))
    }

    #[must_use]
    pub fn has_pending_popup(&self) -> bool {
        !self.state.borrow().pending_popups.is_empty()
    }

    #[must_use]
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor.get()
    }

    #[must_use]
    pub fn output_size(&self) -> PhysicalSize {
        self.state.borrow().display_metrics.borrow().output_size()
    }

    pub fn update_scale_factor(&self, scale_factor: f32) {
        self.scale_factor.set(scale_factor);

        let render_scale = FractionalScaleConfig::render_scale(scale_factor);
        for popup in self.state.borrow().popups.values() {
            popup.window.set_scale_factor(render_scale);
        }
        self.mark_all_popups_dirty();
    }

    pub fn update_output_size(&self, output_size: PhysicalSize) {
        self.state
            .borrow()
            .display_metrics
            .borrow_mut()
            .update_output_size(output_size);
    }

    pub fn create_pending_popup(
        self: &Rc<Self>,
        queue_handle: &QueueHandle<AppState>,
        parent_layer_surface: &ZwlrLayerSurfaceV1,
        last_pointer_serial: u32,
    ) -> Result<Rc<PopupWindow>> {
        let (id, config, width, height) = self.take_pending_popup_params().ok_or_else(|| {
            LayerShikaError::WindowConfiguration {
                message: "No pending popup request available".into(),
            }
        })?;

        let params = CreatePopupParams {
            last_pointer_serial,
            width,
            height,
            position: config.position.clone(),
            constraint_adjustment: config.behavior.constraint_adjustment,
            grab: config.behavior.grab,
        };

        self.create_popup_internal(queue_handle, parent_layer_surface, &params, id)
    }

    fn create_popup_internal(
        self: &Rc<Self>,
        queue_handle: &QueueHandle<AppState>,
        parent_layer_surface: &ZwlrLayerSurfaceV1,
        params: &CreatePopupParams,
        popup_id: PopupId,
    ) -> Result<Rc<PopupWindow>> {
        let xdg_wm_base = self.context.xdg_wm_base.as_ref().ok_or_else(|| {
            LayerShikaError::WindowConfiguration {
                message: "xdg-shell not available for popups".into(),
            }
        })?;

        let scale_factor = self.scale_factor();
        logger::info!(
            "Creating popup window with scale factor {scale_factor}, size=({} x {}), position={:?}",
            params.width,
            params.height,
            params.position
        );

        let output_size = self.output_size();
        #[allow(clippy::cast_precision_loss)]
        let output_logical_size = DomainLogicalSize::from_raw(
            output_size.width as f32 / scale_factor,
            output_size.height as f32 / scale_factor,
        );

        let popup_logical_size = DomainLogicalSize::from_raw(params.width, params.height);
        let domain_scale = self
            .state
            .borrow()
            .display_metrics
            .borrow()
            .scale_factor_typed();
        let popup_dimensions = SurfaceDimensions::from_logical(popup_logical_size, domain_scale);

        let popup_size = PhysicalSize::new(
            popup_dimensions.physical_width(),
            popup_dimensions.physical_height(),
        );

        logger::info!("Popup physical size: {popup_size:?}");

        let wayland_popup_surface =
            PopupSurface::create(&super::popup_surface::PopupSurfaceParams {
                compositor: &self.context.compositor,
                xdg_wm_base,
                parent_layer_surface,
                fractional_scale_manager: self.context.fractional_scale_manager.as_ref(),
                viewporter: self.context.viewporter.as_ref(),
                queue_handle,
                position: params.position.clone(),
                output_bounds: output_logical_size,
                constraint_adjustment: params.constraint_adjustment,
                physical_size: popup_size,
                scale_factor,
            });

        if params.grab {
            wayland_popup_surface.grab(&self.context.seat, params.last_pointer_serial);
        } else {
            logger::info!("Skipping popup grab (grab disabled in request)");
            wayland_popup_surface.surface.commit();
        }

        let context = self
            .context
            .render_factory
            .create_context(&wayland_popup_surface.surface.id(), popup_size)?;

        let renderer = FemtoVGRenderer::new(context)
            .map_err(|e| LayerShikaError::FemtoVGRendererCreation { source: e })?;

        let on_close: OnCloseCallback = {
            let weak_self = Rc::downgrade(self);
            Box::new(move |handle: PopupHandle| {
                if let Some(manager) = weak_self.upgrade() {
                    let id = PopupId::from_handle(handle);
                    manager.destroy_popup(id);
                }
            })
        };

        let popup_window = PopupWindow::new_with_callback(renderer, on_close);
        popup_window.set_popup_id(popup_id.to_handle());

        let config = FractionalScaleConfig::new(params.width, params.height, scale_factor);
        logger::info!(
            "Popup using render scale {} (from {}), render_physical {}x{}",
            config.render_scale,
            scale_factor,
            config.render_physical_size.width,
            config.render_physical_size.height
        );
        config.apply_to(popup_window.as_ref());

        let mut state = self.state.borrow_mut();
        state.popups.insert(
            popup_id,
            ActivePopup {
                surface: wayland_popup_surface,
                window: Rc::clone(&popup_window),
            },
        );

        logger::info!("Popup window created successfully with id {:?}", popup_id);

        Ok(popup_window)
    }

    pub fn render_popups(&self) -> Result<()> {
        let state = self.state.borrow();
        for popup in state.popups.values() {
            popup.window.render_frame_if_dirty()?;
        }
        Ok(())
    }

    pub const fn has_xdg_shell(&self) -> bool {
        self.context.xdg_wm_base.is_some()
    }

    pub fn mark_all_popups_dirty(&self) {
        let state = self.state.borrow();
        for popup in state.popups.values() {
            popup.window.request_redraw();
        }
    }

    pub fn find_popup_key_by_surface_id(&self, surface_id: &ObjectId) -> Option<usize> {
        self.state
            .borrow()
            .popups
            .iter()
            .find_map(|(id, popup)| (popup.surface.surface.id() == *surface_id).then_some(id.key()))
    }

    pub fn find_popup_key_by_fractional_scale_id(
        &self,
        fractional_scale_id: &ObjectId,
    ) -> Option<usize> {
        self.state.borrow().popups.iter().find_map(|(id, popup)| {
            popup
                .surface
                .fractional_scale
                .as_ref()
                .filter(|fs| fs.id() == *fractional_scale_id)
                .map(|_| id.key())
        })
    }

    pub fn get_popup_window(&self, key: usize) -> Option<Rc<PopupWindow>> {
        let id = PopupId(key);
        self.state
            .borrow()
            .popups
            .get(&id)
            .map(|popup| Rc::clone(&popup.window))
    }

    fn destroy_popup(&self, id: PopupId) {
        if let Some(_popup) = self.state.borrow_mut().popups.remove(&id) {
            logger::info!("Destroying popup with id {:?}", id);
            // cleanup happens automatically via ActivePopup::drop()
        }
    }

    pub fn find_popup_key_by_xdg_popup_id(&self, xdg_popup_id: &ObjectId) -> Option<usize> {
        self.state.borrow().popups.iter().find_map(|(id, popup)| {
            (popup.surface.xdg_popup.id() == *xdg_popup_id).then_some(id.key())
        })
    }

    pub fn find_popup_key_by_xdg_surface_id(&self, xdg_surface_id: &ObjectId) -> Option<usize> {
        self.state.borrow().popups.iter().find_map(|(id, popup)| {
            (popup.surface.xdg_surface.id() == *xdg_surface_id).then_some(id.key())
        })
    }

    pub fn update_popup_viewport(&self, key: usize, logical_width: i32, logical_height: i32) {
        let id = PopupId(key);
        if let Some(popup) = self.state.borrow().popups.get(&id) {
            popup.window.begin_repositioning();
            popup
                .surface
                .update_viewport_size(logical_width, logical_height);
        }
    }

    pub fn commit_popup_surface(&self, key: usize) {
        let id = PopupId(key);
        if let Some(popup) = self.state.borrow().popups.get(&id) {
            popup.surface.surface.commit();
            popup.window.end_repositioning();
            popup.window.request_redraw();
        }
    }

    pub fn mark_popup_configured(&self, key: usize) {
        let id = PopupId(key);
        if let Some(popup) = self.state.borrow().popups.get(&id) {
            popup.window.mark_configured();
        }
    }

    pub fn close(&self, handle: PopupHandle) -> Result<()> {
        let id = PopupId::from_handle(handle);
        self.destroy_popup(id);
        Ok(())
    }

    #[must_use]
    pub fn find_by_surface(&self, surface_id: &ObjectId) -> Option<PopupHandle> {
        self.find_popup_key_by_surface_id(surface_id)
            .map(PopupHandle::from_raw)
    }

    #[must_use]
    pub fn find_by_fractional_scale(&self, fractional_scale_id: &ObjectId) -> Option<PopupHandle> {
        self.find_popup_key_by_fractional_scale_id(fractional_scale_id)
            .map(PopupHandle::from_raw)
    }

    #[must_use]
    pub fn find_by_xdg_popup(&self, xdg_popup_id: &ObjectId) -> Option<PopupHandle> {
        self.find_popup_key_by_xdg_popup_id(xdg_popup_id)
            .map(PopupHandle::from_raw)
    }

    #[must_use]
    pub fn find_by_xdg_surface(&self, xdg_surface_id: &ObjectId) -> Option<PopupHandle> {
        self.find_popup_key_by_xdg_surface_id(xdg_surface_id)
            .map(PopupHandle::from_raw)
    }

    #[must_use]
    pub fn get_active_window(
        &self,
        surface: &WlSurface,
        main_surface_id: &ObjectId,
    ) -> ActiveWindow {
        let surface_id = surface.id();

        if *main_surface_id == surface_id {
            return ActiveWindow::Main;
        }

        if let Some(popup_handle) = self
            .find_popup_key_by_surface_id(&surface_id)
            .map(PopupHandle::from_raw)
        {
            return ActiveWindow::Popup(popup_handle);
        }

        ActiveWindow::None
    }

    pub fn update_scale_for_fractional_scale_object(
        &self,
        fractional_scale_proxy: &WpFractionalScaleV1,
        scale_120ths: u32,
    ) {
        let fractional_scale_id = fractional_scale_proxy.id();

        if let Some(popup_key) = self.find_popup_key_by_fractional_scale_id(&fractional_scale_id) {
            if let Some(popup_surface) = self.get_popup_window(popup_key) {
                let new_scale_factor = DisplayMetrics::scale_factor_from_120ths(scale_120ths);
                let render_scale = FractionalScaleConfig::render_scale(new_scale_factor);
                logger::info!(
                    "Updating popup scale factor to {} (render scale {}, from {}x)",
                    new_scale_factor,
                    render_scale,
                    scale_120ths
                );
                popup_surface.set_scale_factor(render_scale);
                popup_surface.request_redraw();
            }
        }
    }
}
