use anyhow::{anyhow, Result};
use log::{debug, error};
use smithay_client_toolkit::reexports::calloop::{self, Interest, Mode, PostAction};
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use wayland_client::{Connection, EventQueue};

use crate::rendering::femtovg_window::FemtoVGWindow;

use super::state::WindowState;

pub struct EventLoopHandler {
    window: Weak<FemtoVGWindow>,
    wayland_queue: Weak<RefCell<EventQueue<WindowState>>>,
    connection: Weak<Connection>,
    state: Weak<RefCell<WindowState>>,
}

impl EventLoopHandler {
    pub fn new(
        window: Weak<FemtoVGWindow>,
        wayland_queue: Weak<RefCell<EventQueue<WindowState>>>,
        connection: Weak<Connection>,
        state: Weak<RefCell<WindowState>>,
    ) -> Self {
        debug!("Creating EventLoopHandler");
        Self {
            window,
            wayland_queue,
            connection,
            state,
        }
    }

    pub fn setup_wayland_event_source(&self, loop_handle: &calloop::LoopHandle<()>) -> Result<()> {
        debug!("Setting up Wayland event source");

        let wayland_queue = Weak::clone(&self.wayland_queue);
        let state = Weak::clone(&self.state);
        let connection = self.connection.upgrade().ok_or_else(|| {
            anyhow!("Failed to get Wayland connection reference in Wayland event source")
        })?;
        let window = Weak::clone(&self.window);

        loop_handle
            .insert_source(
                calloop::generic::Generic::new(connection, Interest::READ, Mode::Level),
                move |_, connection, ()| {
                    let result: Result<PostAction, anyhow::Error> = (|| {
                        let wayland_queue = wayland_queue
                            .upgrade()
                            .ok_or_else(|| anyhow!("Failed to get Wayland queue reference"))?;
                        let state = state
                            .upgrade()
                            .ok_or_else(|| anyhow!("Failed to get event handler reference"))?;
                        let window = window
                            .upgrade()
                            .ok_or_else(|| anyhow!("Failed to get window reference"))?;
                        Self::handle_wayland_events(connection, &wayland_queue, &state, &window)?;
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
            .dispatch_pending(&mut *state.borrow_mut())
            .map_err(|e| anyhow!("Failed to dispatch Wayland events: {}", e))?;

        slint::platform::update_timers_and_animations();
        window.render_frame_if_dirty();
        Ok(())
    }
}
