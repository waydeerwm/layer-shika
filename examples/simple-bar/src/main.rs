use std::path::PathBuf;

use layer_shika::prelude::*;

fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    log::info!("Starting simple-bar example");

    let ui_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/bar.slint");

    Shell::from_file(ui_path)
        .surface("Bar")
        .height(42)
        .anchor(AnchorEdges::top_bar())
        .exclusive_zone(42)
        .namespace("simple-bar-example")
        .build()?
        .run()?;

    Ok(())
}
