use anyhow::{anyhow, Result};
use log::{debug, error};
use smithay_client_toolkit::reexports::calloop::{self, Interest, Mode, PostAction};
use std::cell::RefCell;
use std::rc::Rc;
use wayland_client::{Connection, EventQueue};

use crate::rendering::femtovg_window::FemtoVGWindow;

use super::state::WindowState;

pub struct EventLoopHandler {
    wayland_queue: Rc<RefCell<EventQueue<WindowState>>>,
    connection: Rc<Connection>,
    state: Rc<RefCell<WindowState>>,
}

impl EventLoopHandler {
    pub fn new(
        wayland_queue: Rc<RefCell<EventQueue<WindowState>>>,
        connection: Rc<Connection>,
        state: Rc<RefCell<WindowState>>,
    ) -> Self {
        debug!("Creating EventLoopHandler");
        Self {
            wayland_queue,
            connection,
            state,
        }
    }

    pub fn setup_wayland_event_source(&self, loop_handle: &calloop::LoopHandle<()>) -> Result<()> {
        debug!("Setting up Wayland event source");

        let wayland_queue = Rc::clone(&self.wayland_queue);
        let state = Rc::clone(&self.state);
        let connection = Rc::clone(&self.connection);

        loop_handle
            .insert_source(
                calloop::generic::Generic::new(connection, Interest::READ, Mode::Level),
                move |_, connection, ()| {
                    let result: Result<PostAction, anyhow::Error> = (|| {
                        let binding = state.borrow().window();
                        let window = binding.as_ref().ok_or_else(|| {
                            anyhow!("Window not initialized in Wayland event source")
                        })?;
                        Self::handle_wayland_events(connection, &wayland_queue, &state, window)?;
                        Ok(PostAction::Continue)
                    })();

                    result.map_err(|e| {
                        error!("Error handling Wayland events: {}", e);
                        std::io::Error::new(std::io::ErrorKind::Other, e)
                    })
                },
            )
            .map_err(|e| anyhow!("Failed to insert Wayland event source: {}", e))?;

        Ok(())
    }

    fn handle_wayland_events(
        connection: &Connection,
        wayland_queue: &Rc<RefCell<EventQueue<WindowState>>>,
        state: &Rc<RefCell<WindowState>>,
        window: &Rc<FemtoVGWindow>,
    ) -> Result<()> {
        connection
            .flush()
            .map_err(|e| anyhow!("Failed to flush connection: {}", e))?;

        let mut event_queue = wayland_queue.borrow_mut();
        if let Some(guard) = event_queue.prepare_read() {
            guard
                .read()
                .map_err(|e| anyhow!("Failed to read Wayland events: {}", e))?;
        }

        event_queue
            .blocking_dispatch(&mut state.borrow_mut())
            .map_err(|e| anyhow!("Failed to dispatch Wayland events: {}", e))?;

        slint::platform::update_timers_and_animations();
        window.render_frame_if_dirty();
        Ok(())
    }
}
