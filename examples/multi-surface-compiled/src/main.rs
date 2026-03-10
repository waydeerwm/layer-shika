use layer_shika::prelude::*;

slint::include_modules!();

fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let platform = LayerShell::new().unwrap();

    platform
        .window("TopBar")
        .namespace("top-bar")
        .anchor(AnchorEdges::top_bar());

    platform
        .window("BottomBar")
        .namespace("bottom-bar")
        .layer(Layer::Overlay)
        .anchor(AnchorEdges::bottom_bar())
        .exclusive_zone(0);

    slint::platform::set_platform(platform).unwrap();

    let top = TopBar::new().unwrap();
    let bottom = BottomBar::new().unwrap();

    top.show().unwrap();
    bottom.run().unwrap();

    Ok(())
}
