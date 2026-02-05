use std::path::PathBuf;
use std::rc::Rc;

use layer_shika::prelude::*;
use layer_shika::slint_interpreter::Value;

fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let ui_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/lock.slint");

    let mut shell = Shell::from_file(ui_path).build()?;

    let lock = Rc::new(shell.create_session_lock("LockScreen")?);

    let lock_clone = Rc::clone(&lock);
    shell.select_lock(Surface::all()).on_callback_with_args(
        "unlock_requested",
        move |args, _ctx| {
            if let Some(password) = args.first() {
                log::info!("Password entered: {:?}", password);
            }
            lock_clone.deactivate().ok();
        },
    );

    shell
        .select_lock(Surface::all())
        .on_callback("cancel_requested", |_ctx| {
            log::info!("Cancel requested button pressed");
        });

    shell
        .select_lock(Surface::all())
        .set_property("theme", &Value::from(slint::SharedString::from("dark")))
        .ok();

    lock.activate()?;

    shell.run()?;

    Ok(())
}
