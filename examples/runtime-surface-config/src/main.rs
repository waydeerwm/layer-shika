use std::path::PathBuf;
use std::rc::Rc;

use layer_shika::calloop::channel::Sender;
use layer_shika::prelude::*;
use layer_shika::slint::SharedString;
use layer_shika::slint_interpreter::Value;

#[derive(Debug)]
enum UiUpdate {
    ToggleSize,
    SwitchAnchor,
    SwitchLayer,
}

fn setup_toggle_size_callback(sender: &Rc<Sender<UiUpdate>>, shell: &Shell) {
    let sender_clone = Rc::clone(sender);
    shell.select(Surface::named("Bar")).on_callback_with_args(
        "toggle-size",
        move |args, control| {
            let is_expanded = args
                .first()
                .and_then(|v| v.clone().try_into().ok())
                .unwrap_or(false);

            let new_size = if is_expanded { 64 } else { 32 };
            let (width, height) = (0, new_size);

            log::info!(
                "Toggling bar size to {}px (expanded: {})",
                new_size,
                is_expanded
            );

            if let Err(e) = control
                .this_instance()
                .configure()
                .size(width, height)
                .exclusive_zone(new_size.try_into().unwrap_or(32))
                .apply()
            {
                log::error!("Failed to apply configuration: {}", e);
            }

            if let Err(e) = sender_clone.send(UiUpdate::ToggleSize) {
                log::error!("Failed to send UI update: {}", e);
            }
        },
    );
}

fn setup_anchor_switch_callback(sender: &Rc<Sender<UiUpdate>>, shell: &Shell) {
    let sender_clone = Rc::clone(sender);
    shell.select(Surface::named("Bar")).on_callback_with_args(
        "switch-anchor",
        move |args, control| {
            let new_anchor = args
                .first()
                .and_then(|v| match v {
                    Value::String(s) => Some(s.as_str()),
                    _ => None,
                })
                .unwrap_or("Top");

            let anchor_edges = match new_anchor {
                "Bottom" => AnchorEdges::bottom_bar(),
                _ => AnchorEdges::top_bar(),
            };

            log::info!("Switching anchor to: {}", new_anchor);

            if let Err(e) = control.this_instance().set_anchor(anchor_edges) {
                log::error!("Failed to apply anchor config: {}", e);
            }

            if let Err(e) = sender_clone.send(UiUpdate::SwitchAnchor) {
                log::error!("Failed to send UI update: {}", e);
            }
        },
    );
}

fn setup_layer_switch_callback(sender: &Rc<Sender<UiUpdate>>, shell: &Shell) {
    let sender_clone = Rc::clone(sender);
    shell.select(Surface::named("Bar")).on_callback_with_args(
        "switch-layer",
        move |args, control| {
            let new_layer_str = args
                .first()
                .and_then(|v| match v {
                    Value::String(s) => Some(s.as_str()),
                    _ => None,
                })
                .unwrap_or("Top");

            let new_layer = match new_layer_str {
                "Background" => Layer::Background,
                "Bottom" => Layer::Bottom,
                "Overlay" => Layer::Overlay,
                _ => Layer::Top,
            };

            log::info!("Switching layer to: {:?}", new_layer);

            if let Err(e) = control.this_instance().set_layer(new_layer) {
                log::error!("Failed to set layer: {}", e);
            }

            if let Err(e) = sender_clone.send(UiUpdate::SwitchLayer) {
                log::error!("Failed to send UI update: {}", e);
            }
        },
    );
}

fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    log::info!("Starting runtime-control example");
    log::info!("This example demonstrates dynamic surface manipulation at runtime");

    let ui_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/bar.slint");

    let mut shell = Shell::from_file(ui_path)
        .surface("Bar")
        .height(32)
        .anchor(AnchorEdges::top_bar())
        .exclusive_zone(32)
        .namespace("runtime-control-example")
        .build()?;

    shell
        .select(Surface::named("Bar"))
        .with_component(|component| {
            log::info!("Initializing properties for Bar surface");

            let set_property = |name: &str, value: Value| {
                if let Err(e) = component.set_property(name, value) {
                    log::error!("Failed to set initial {}: {}", name, e);
                }
            };

            set_property("is-expanded", false.into());
            set_property("current-anchor", SharedString::from("Top").into());
            set_property("current-layer", SharedString::from("Top").into());

            log::info!("Initialized properties for Bar surface");
        });

    let handle = shell.event_loop_handle();
    let (_token, sender) = handle.add_channel(|message: UiUpdate, _app_state| {
        log::info!("Received UI update: {:?}", message);
    })?;

    let sender_rc = Rc::new(sender);

    setup_toggle_size_callback(&sender_rc, &shell);
    setup_anchor_switch_callback(&sender_rc, &shell);
    setup_layer_switch_callback(&sender_rc, &shell);
    shell.run()?;

    Ok(())
}
