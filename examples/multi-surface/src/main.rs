use std::path::PathBuf;

use layer_shika::prelude::*;
use layer_shika::slint_interpreter::Value;

fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    log::info!("Starting multi-surface example");

    let ui_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/shell.slint");

    let mut shell = Shell::from_file(ui_path)
        .surface("TopBar")
        .height(42)
        .anchor(AnchorEdges::top_bar())
        .exclusive_zone(42)
        .namespace("multi-surface-top")
        .surface("Dock")
        .height(64)
        .anchor(AnchorEdges::bottom_bar())
        .exclusive_zone(64)
        .namespace("multi-surface-dock")
        .build()?;

    shell
        .select(Surface::named("TopBar"))
        .on_callback("workspace_clicked", |_control| {
            log::info!("Workspace button clicked in TopBar");
        });

    shell
        .select(Surface::named("Dock"))
        .on_callback_with_args("app_clicked", |args, _control| {
            if let Some(Value::String(app_name)) = args.first() {
                log::info!("App clicked in Dock: {}", app_name.as_str());
            }
        });

    log::info!("Running shell with surfaces: {:?}", shell.surface_names());

    shell.run()?;

    Ok(())
}
