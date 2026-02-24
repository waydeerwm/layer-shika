use std::path::PathBuf;

use layer_shika::prelude::*;
use layer_shika::slint_interpreter::Value;

fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    log::info!("Starting surface-input-region example");

    let ui_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/ui.slint");

    let compiled = Shell::compile_file(&ui_path)?;

    let mut shell = Shell::from_compilation(compiled)
        .surface("MainWindow")
        .height(64)
        .anchor(AnchorEdges::top_bar())
        .exclusive_zone(32)
        .namespace("surface-input-region-example")
        .build()?;

    // Hacky way to get a surface size because `init` callback is called before Shell::run()
    shell
        .select(Surface::named("MainWindow"))
        .on_callback_with_args("width-changed", |args, ctx| {
            let Some(Value::Number(width)) = args.first() else {
                log::error!("MainWindow.width-changed provided no width");
                return;
            };

            #[allow(clippy::cast_possible_truncation)]
            let width_i32 = *width as i32;

            if let Err(e) = ctx
                .control()
                .surface("MainWindow")
                .set_input_region(0, 0, width_i32, 32)
            {
                log::error!("Failed to set_input_region: {e}");
            }
        });

    shell.run()?;
    Ok(())
}
