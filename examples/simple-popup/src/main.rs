use std::path::PathBuf;

use layer_shika::prelude::*;

fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    log::info!("Starting simple-popup example");

    let ui_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/ui.slint");

    let mut shell = Shell::from_file(ui_path)
        .surface("MainWindow")
        .height(42)
        .anchor(AnchorEdges::top_bar())
        .exclusive_zone(42)
        .namespace("simple-popup-example")
        .build()?;

    shell
        .select(Surface::named("MainWindow"))
        .on_callback("open_popup", |ctx| {
            if let Err(e) = ctx
                .popups()
                .builder("ExamplePopup")
                .at_cursor()
                .content_sized()
                .grab(true)
                .close_on("close_popup")
                .resize_on("resize_popup")
                .show()
            {
                log::error!("Failed to show popup: {e}");
            }
        });

    shell
        .select(Surface::named("MainWindow"))
        .on_callback("open_two_popups", |ctx| {
            if let Err(e) = ctx
                .popups()
                .builder("ExamplePopup")
                .at_cursor()
                .content_sized()
                .grab(false)
                .close_on("close_popup")
                .resize_on("resize_popup")
                .show()
            {
                log::error!("Failed to show first popup: {e}");
            }

            if let Err(e) = ctx
                .popups()
                .builder("ExamplePopup")
                .centered()
                .fixed_size(360.0, 140.0)
                .grab(false)
                .close_on("close_popup")
                .show()
            {
                log::error!("Failed to show second popup: {e}");
            }
        });

    shell.run()?;
    Ok(())
}
