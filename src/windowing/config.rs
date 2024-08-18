use slint_interpreter::ComponentDefinition;
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{self},
    zwlr_layer_surface_v1::{Anchor, KeyboardInteractivity},
};

pub struct WindowConfig {
    pub height: u32,
    pub layer: zwlr_layer_shell_v1::Layer,
    pub margin: (i32, i32, i32, i32),
    pub anchor: Anchor,
    pub keyboard_interactivity: KeyboardInteractivity,
    pub exclusive_zone: i32,
    pub scale_factor: f32,
    pub namespace: String,
    pub component_definition: Option<ComponentDefinition>,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            height: 30,
            layer: zwlr_layer_shell_v1::Layer::Top,
            margin: (0, 0, 0, 0),
            anchor: Anchor::Top | Anchor::Left | Anchor::Right,
            keyboard_interactivity: KeyboardInteractivity::OnDemand,
            exclusive_zone: -1,
            namespace: "layer-shika".to_owned(),
            scale_factor: 1.0,
            component_definition: None,
        }
    }
}
