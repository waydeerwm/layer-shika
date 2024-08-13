use anyhow::{anyhow, Result};
use log::{debug, error, info};
use smithay_client_toolkit::reexports::calloop::{self, EventLoop, Interest, Mode, PostAction};
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use wayland_client::{Connection, EventQueue};

use crate::rendering::femtovg_window::FemtoVGWindow;
use crate::windowing::event_handler::WindowEventHandler;

pub struct EventLoopHandler {
    window: Weak<FemtoVGWindow>,
    wayland_queue: Weak<RefCell<EventQueue<WindowEventHandler>>>,
    connection: Weak<Connection>,
    event_handler: Weak<RefCell<WindowEventHandler>>,
}

impl EventLoopHandler {
    pub fn new(
        window: Weak<FemtoVGWindow>,
        wayland_queue: Weak<RefCell<EventQueue<WindowEventHandler>>>,
        connection: Weak<Connection>,
        event_handler: Weak<RefCell<WindowEventHandler>>,
    ) -> Self {
        debug!("Creating EventLoopHandler");
        Self {
            window,
            wayland_queue,
            connection,
            event_handler,
        }
    }

    pub fn setup_wayland_event_source(&self, loop_handle: &calloop::LoopHandle<()>) -> Result<()> {
        debug!("Setting up Wayland event source");

        let wayland_queue = self.wayland_queue.clone();
        let event_handler = self.event_handler.clone();
        let connection = self.connection.upgrade().ok_or_else(|| {
            anyhow!("Failed to get Wayland connection reference in Wayland event source")
        })?;
        let window = self.window.clone();

        loop_handle
            .insert_source(
                calloop::generic::Generic::new(connection, Interest::READ, Mode::Level),
                move |_, connection, _| {
                    let result: Result<PostAction, anyhow::Error> = (|| {
                        let wayland_queue = wayland_queue
                            .upgrade()
                            .ok_or_else(|| anyhow!("Failed to get Wayland queue reference"))?;
                        let event_handler = event_handler
                            .upgrade()
                            .ok_or_else(|| anyhow!("Failed to get event handler reference"))?;
                        let window = window
                            .upgrade()
                            .ok_or_else(|| anyhow!("Failed to get window reference"))?;
                        Self::handle_wayland_events(
                            connection,
                            &wayland_queue,
                            &event_handler,
                            &window,
                        )?;
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

    pub fn run(&self, event_loop: &mut EventLoop<()>) -> Result<()> {
        info!("Starting event loop");
        event_loop
            .run(None, &mut (), |_| {})
            .map_err(|e| anyhow!("Failed to run event loop: {}", e))
    }

    fn handle_wayland_events(
        connection: &Connection,
        wayland_queue: &Rc<RefCell<EventQueue<WindowEventHandler>>>,
        event_handler: &Rc<RefCell<WindowEventHandler>>,
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
            .dispatch_pending(&mut *event_handler.borrow_mut())
            .map_err(|e| anyhow!("Failed to dispatch Wayland events: {}", e))?;

        slint::platform::update_timers_and_animations();
        window.render_frame_if_dirty();
        Ok(())
    }
}
