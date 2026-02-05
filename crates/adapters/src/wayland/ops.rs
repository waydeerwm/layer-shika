use std::rc::Rc;

use layer_shika_domain::value_objects::lock_config::LockConfig;
use layer_shika_domain::value_objects::lock_state::LockState;
use layer_shika_domain::value_objects::output_handle::OutputHandle;
use slint_interpreter::{CompilationResult, ComponentInstance, Value};
use smithay_client_toolkit::reexports::calloop::LoopHandle;

use crate::errors::Result;
use crate::wayland::config::ShellSurfaceConfig;
use crate::wayland::session_lock::{LockPropertyOperation, OutputFilter};
use crate::wayland::surfaces::app_state::AppState;

type SessionLockCallback = Rc<dyn Fn(&[Value]) -> Value>;

pub trait WaylandSystemOps {
    fn run(&mut self) -> Result<()>;

    fn spawn_surface(&mut self, config: &ShellSurfaceConfig) -> Result<Vec<OutputHandle>>;

    fn despawn_surface(&mut self, name: &str) -> Result<()>;

    fn set_compilation_result(&mut self, compilation_result: Rc<CompilationResult>);

    fn activate_session_lock(&mut self, component_name: &str, config: LockConfig) -> Result<()>;

    fn deactivate_session_lock(&mut self) -> Result<()>;

    fn is_session_lock_available(&self) -> bool;

    fn session_lock_state(&self) -> Option<LockState>;

    fn register_session_lock_callback(&mut self, callback_name: &str, handler: SessionLockCallback);

    fn register_session_lock_callback_with_filter(
        &mut self,
        callback_name: &str,
        handler: SessionLockCallback,
        filter: OutputFilter,
    );

    fn register_session_lock_property_operation(
        &mut self,
        property_operation: LockPropertyOperation,
    );

    fn session_lock_component_name(&self) -> Option<String>;

    fn iter_lock_surfaces(&self, f: &mut dyn FnMut(OutputHandle, &ComponentInstance));

    fn count_lock_surfaces(&self) -> usize;

    fn app_state(&self) -> &AppState;

    fn app_state_mut(&mut self) -> &mut AppState;

    fn event_loop_handle(&self) -> LoopHandle<'static, AppState>;

    fn component_instance(&self) -> Result<&ComponentInstance>;
}
