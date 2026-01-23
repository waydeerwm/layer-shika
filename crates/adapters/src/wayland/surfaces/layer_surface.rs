use crate::{
    logger,
    wayland::{config::LayerSurfaceConfig, surfaces::app_state::AppState},
};
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{Layer, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
};
use std::rc::Rc;
use wayland_client::{
    QueueHandle,
    protocol::{wl_compositor::WlCompositor, wl_output::WlOutput, wl_surface::WlSurface},
};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::WpFractionalScaleV1,
};
use wayland_protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};

pub struct SurfaceSetupParams<'a> {
    pub compositor: &'a WlCompositor,
    pub output: &'a WlOutput,
    pub layer_shell: &'a ZwlrLayerShellV1,
    pub fractional_scale_manager: Option<&'a WpFractionalScaleManagerV1>,
    pub viewporter: Option<&'a WpViewporter>,
    pub queue_handle: &'a QueueHandle<AppState>,
    pub layer: Layer,
    pub namespace: String,
}

pub struct SurfaceCtx {
    pub surface: Rc<WlSurface>,
    pub layer_surface: Rc<ZwlrLayerSurfaceV1>,
    pub fractional_scale: Option<Rc<WpFractionalScaleV1>>,
    pub viewport: Option<Rc<WpViewport>>,
}

impl SurfaceCtx {
    pub(crate) fn setup(
        setup_params: &SurfaceSetupParams<'_>,
        config: &LayerSurfaceConfig,
    ) -> Self {
        let surface = Rc::new(
            setup_params
                .compositor
                .create_surface(setup_params.queue_handle, ()),
        );
        let layer_surface = Rc::new(setup_params.layer_shell.get_layer_surface(
            &surface,
            Some(setup_params.output),
            setup_params.layer,
            setup_params.namespace.clone(),
            setup_params.queue_handle,
            (),
        ));

        let fractional_scale = setup_params.fractional_scale_manager.map(|manager| {
            logger::info!("Creating fractional scale object for surface");
            Rc::new(manager.get_fractional_scale(&surface, setup_params.queue_handle, ()))
        });

        let viewport = setup_params.viewporter.map(|vp| {
            logger::info!("Creating viewport for surface");
            Rc::new(vp.get_viewport(&surface, setup_params.queue_handle, ()))
        });

        Self::configure_layer_surface(&layer_surface, &surface, config);
        surface.set_buffer_scale(1);

        Self {
            surface,
            layer_surface,
            fractional_scale,
            viewport,
        }
    }

    fn configure_layer_surface(
        layer_surface: &Rc<ZwlrLayerSurfaceV1>,
        surface: &WlSurface,
        config: &LayerSurfaceConfig,
    ) {
        layer_surface.set_anchor(config.anchor);
        layer_surface.set_margin(
            config.margin.top,
            config.margin.right,
            config.margin.bottom,
            config.margin.left,
        );

        layer_surface.set_exclusive_zone(config.exclusive_zone);
        layer_surface.set_keyboard_interactivity(config.keyboard_interactivity);

        layer_surface.set_size(config.width, config.height);
        surface.commit();
    }
}
