use layer_shika_domain::dimensions::{LogicalRect, LogicalSize as DomainLogicalSize};
use layer_shika_domain::value_objects::popup_behavior::ConstraintAdjustment as DomainConstraintAdjustment;
use layer_shika_domain::value_objects::popup_position::{Alignment, AnchorPoint, PopupPosition};
use slint::PhysicalSize;
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1;
use std::rc::Rc;
use wayland_client::{
    protocol::{wl_compositor::WlCompositor, wl_seat::WlSeat, wl_surface::WlSurface},
    QueueHandle,
};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::WpFractionalScaleV1,
};
use wayland_protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};
use wayland_protocols::xdg::shell::client::{
    xdg_popup::XdgPopup,
    xdg_positioner::{Anchor, ConstraintAdjustment, Gravity, XdgPositioner},
    xdg_surface::XdgSurface,
    xdg_wm_base::XdgWmBase,
};

use crate::logger;

use super::app_state::AppState;

pub struct PopupSurfaceParams<'a> {
    pub compositor: &'a WlCompositor,
    pub xdg_wm_base: &'a XdgWmBase,
    pub parent_layer_surface: &'a ZwlrLayerSurfaceV1,
    pub fractional_scale_manager: Option<&'a WpFractionalScaleManagerV1>,
    pub viewporter: Option<&'a WpViewporter>,
    pub queue_handle: &'a QueueHandle<AppState>,
    pub position: PopupPosition,
    pub output_bounds: DomainLogicalSize,
    pub constraint_adjustment: DomainConstraintAdjustment,
    pub physical_size: PhysicalSize,
    pub scale_factor: f32,
}

pub struct PopupSurface {
    pub surface: Rc<WlSurface>,
    pub xdg_surface: Rc<XdgSurface>,
    pub xdg_popup: Rc<XdgPopup>,
    pub fractional_scale: Option<Rc<WpFractionalScaleV1>>,
    pub viewport: Option<Rc<WpViewport>>,
    position: PopupPosition,
    output_bounds: DomainLogicalSize,
    constraint_adjustment: DomainConstraintAdjustment,
    xdg_wm_base: Rc<XdgWmBase>,
    queue_handle: QueueHandle<AppState>,
}

impl PopupSurface {
    pub fn create(params: &PopupSurfaceParams<'_>) -> Self {
        let surface = Rc::new(params.compositor.create_surface(params.queue_handle, ()));

        let xdg_surface = Rc::new(params.xdg_wm_base.get_xdg_surface(
            &surface,
            params.queue_handle,
            (),
        ));

        let positioner = Self::create_positioner(params);

        let xdg_popup = Rc::new(xdg_surface.get_popup(None, &positioner, params.queue_handle, ()));

        logger::info!("Attaching popup to layer surface via get_popup");
        params.parent_layer_surface.get_popup(&xdg_popup);

        let fractional_scale = params.fractional_scale_manager.map(|manager| {
            logger::info!("Creating fractional scale object for popup surface");
            Rc::new(manager.get_fractional_scale(&surface, params.queue_handle, ()))
        });

        let viewport = params.viewporter.map(|vp| {
            logger::info!("Creating viewport for popup surface");
            Rc::new(vp.get_viewport(&surface, params.queue_handle, ()))
        });

        #[allow(clippy::cast_possible_wrap)]
        #[allow(clippy::cast_precision_loss)]
        #[allow(clippy::cast_possible_truncation)]
        if let Some(ref vp) = viewport {
            let logical_width = (params.physical_size.width as f32 / params.scale_factor) as i32;
            let logical_height = (params.physical_size.height as f32 / params.scale_factor) as i32;
            logger::info!(
                "Setting viewport destination to logical size: {}x{} (physical: {}x{}, scale: {})",
                logical_width,
                logical_height,
                params.physical_size.width,
                params.physical_size.height,
                params.scale_factor
            );
            vp.set_destination(logical_width, logical_height);
        }

        surface.set_buffer_scale(1);

        Self {
            surface,
            xdg_surface,
            xdg_popup,
            fractional_scale,
            viewport,
            position: params.position.clone(),
            output_bounds: params.output_bounds,
            constraint_adjustment: params.constraint_adjustment,
            xdg_wm_base: Rc::new(params.xdg_wm_base.clone()),
            queue_handle: params.queue_handle.clone(),
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_possible_wrap)]
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_precision_loss)]
    fn create_positioner(params: &PopupSurfaceParams<'_>) -> XdgPositioner {
        let positioner = params
            .xdg_wm_base
            .create_positioner(params.queue_handle, ());

        let logical_width = params.physical_size.width as f32 / params.scale_factor;
        let logical_height = params.physical_size.height as f32 / params.scale_factor;

        let (calculated_x, calculated_y) = compute_top_left(
            &params.position,
            DomainLogicalSize::from_raw(logical_width, logical_height),
            params.output_bounds,
        );

        logger::info!(
            "Popup positioning: position={:?}, calculated_top_left=({}, {})",
            params.position,
            calculated_x,
            calculated_y
        );

        let logical_width_i32 = logical_width as i32;
        let logical_height_i32 = logical_height as i32;

        positioner.set_anchor_rect(calculated_x, calculated_y, 1, 1);
        positioner.set_size(logical_width_i32, logical_height_i32);
        positioner.set_anchor(Anchor::TopLeft);
        positioner.set_gravity(Gravity::BottomRight);
        positioner
            .set_constraint_adjustment(map_constraint_adjustment(params.constraint_adjustment));

        positioner
    }

    pub fn grab(&self, seat: &WlSeat, serial: u32) {
        logger::info!("Grabbing popup with serial {serial}");
        self.xdg_popup.grab(seat, serial);

        logger::info!("Committing popup surface to trigger configure event");
        self.surface.commit();
    }

    pub fn update_viewport_size(&self, logical_width: i32, logical_height: i32) {
        if let Some(ref vp) = self.viewport {
            logger::debug!(
                "Updating popup viewport destination to logical size: {}x{}",
                logical_width,
                logical_height
            );
            vp.set_destination(logical_width, logical_height);

            self.reposition_popup(logical_width, logical_height);
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_possible_wrap)]
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_precision_loss)]
    fn reposition_popup(&self, logical_width: i32, logical_height: i32) {
        let (calculated_x, calculated_y) = compute_top_left(
            &self.position,
            DomainLogicalSize::from_raw(logical_width as f32, logical_height as f32),
            self.output_bounds,
        );

        logger::info!(
            "Repositioning popup: new_size=({}x{}), new_top_left=({}, {})",
            logical_width,
            logical_height,
            calculated_x,
            calculated_y
        );

        let positioner = self.xdg_wm_base.create_positioner(&self.queue_handle, ());
        positioner.set_anchor_rect(calculated_x, calculated_y, 1, 1);
        positioner.set_size(logical_width, logical_height);
        positioner.set_anchor(Anchor::TopLeft);
        positioner.set_gravity(Gravity::BottomRight);
        positioner.set_constraint_adjustment(map_constraint_adjustment(self.constraint_adjustment));

        self.xdg_popup.reposition(&positioner, 0);
    }

    pub fn destroy(&self) {
        logger::info!("Destroying popup surface");
        self.xdg_popup.destroy();
        self.xdg_surface.destroy();
        self.surface.destroy();
    }
}

fn compute_top_left(
    position: &PopupPosition,
    popup_size: DomainLogicalSize,
    output_bounds: DomainLogicalSize,
) -> (i32, i32) {
    let (mut x, mut y) = match position {
        PopupPosition::Absolute { x, y } => (*x, *y),
        PopupPosition::Centered { offset } => (
            (output_bounds.width() / 2.0) - (popup_size.width() / 2.0) + offset.x,
            (output_bounds.height() / 2.0) - (popup_size.height() / 2.0) + offset.y,
        ),
        PopupPosition::Element {
            rect,
            anchor,
            alignment,
        } => {
            let (anchor_x, anchor_y) = anchor_point_in_rect(*rect, *anchor);
            let (ax, ay) = alignment_offsets(*alignment, popup_size);
            (anchor_x - ax, anchor_y - ay)
        }
        PopupPosition::Cursor { .. } | PopupPosition::RelativeToParent { .. } => {
            logger::warn!("PopupPosition variant not supported by current backend: {position:?}");
            (0.0, 0.0)
        }
    };

    let max_x = (output_bounds.width() - popup_size.width()).max(0.0);
    let max_y = (output_bounds.height() - popup_size.height()).max(0.0);

    x = x.clamp(0.0, max_x);
    y = y.clamp(0.0, max_y);

    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_possible_wrap)]
    (x as i32, y as i32)
}

fn anchor_point_in_rect(rect: LogicalRect, anchor: AnchorPoint) -> (f32, f32) {
    let x = rect.x();
    let y = rect.y();
    let w = rect.width();
    let h = rect.height();

    match anchor {
        AnchorPoint::TopLeft => (x, y),
        AnchorPoint::TopCenter => (x + w / 2.0, y),
        AnchorPoint::TopRight => (x + w, y),
        AnchorPoint::CenterLeft => (x, y + h / 2.0),
        AnchorPoint::Center => (x + w / 2.0, y + h / 2.0),
        AnchorPoint::CenterRight => (x + w, y + h / 2.0),
        AnchorPoint::BottomLeft => (x, y + h),
        AnchorPoint::BottomCenter => (x + w / 2.0, y + h),
        AnchorPoint::BottomRight => (x + w, y + h),
    }
}

fn alignment_offsets(alignment: Alignment, popup_size: DomainLogicalSize) -> (f32, f32) {
    let x = match alignment {
        Alignment::Start => 0.0,
        Alignment::Center => popup_size.width() / 2.0,
        Alignment::End => popup_size.width(),
    };
    let y = match alignment {
        Alignment::Start => 0.0,
        Alignment::Center => popup_size.height() / 2.0,
        Alignment::End => popup_size.height(),
    };
    (x, y)
}

fn map_constraint_adjustment(adjustment: DomainConstraintAdjustment) -> ConstraintAdjustment {
    match adjustment {
        DomainConstraintAdjustment::None => ConstraintAdjustment::None,
        DomainConstraintAdjustment::Slide => {
            ConstraintAdjustment::SlideX | ConstraintAdjustment::SlideY
        }
        DomainConstraintAdjustment::Flip => {
            ConstraintAdjustment::FlipX | ConstraintAdjustment::FlipY
        }
        DomainConstraintAdjustment::Resize => {
            ConstraintAdjustment::ResizeX | ConstraintAdjustment::ResizeY
        }
        DomainConstraintAdjustment::SlideAndResize => {
            ConstraintAdjustment::SlideX
                | ConstraintAdjustment::SlideY
                | ConstraintAdjustment::ResizeX
                | ConstraintAdjustment::ResizeY
        }
        DomainConstraintAdjustment::FlipAndSlide => {
            ConstraintAdjustment::FlipX
                | ConstraintAdjustment::FlipY
                | ConstraintAdjustment::SlideX
                | ConstraintAdjustment::SlideY
        }
        DomainConstraintAdjustment::All => ConstraintAdjustment::all(),
    }
}
