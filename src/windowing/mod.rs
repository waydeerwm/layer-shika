use self::state::WindowState;
use crate::{
    bind_globals,
    rendering::{egl_context::EGLContext, femtovg_window::FemtoVGWindow},
};
use anyhow::{Context, Result};
use config::WindowConfig;
use log::{debug, error, info};
use slint::{platform::femtovg_renderer::FemtoVGRenderer, LogicalPosition, PhysicalSize};
use slint_interpreter::ComponentInstance;
use smithay_client_toolkit::reexports::{
    calloop::{self, EventLoop, Interest, LoopHandle, Mode, PostAction},
    protocols_wlr::layer_shell::v1::client::{
        zwlr_layer_shell_v1::ZwlrLayerShellV1, zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    },
};
use state::builder::WindowStateBuilder;
use std::rc::Rc;
use wayland_client::{
    globals::registry_queue_init,
    protocol::{
        wl_compositor::WlCompositor, wl_display::WlDisplay, wl_output::WlOutput, wl_seat::WlSeat,
        wl_surface::WlSurface,
    },
    Connection, EventQueue, Proxy, QueueHandle,
};

pub mod builder;
mod config;
mod macros;
mod state;

pub struct WindowingSystem {
    state: WindowState,
    connection: Rc<Connection>,
    event_queue: EventQueue<WindowState>,
    event_loop: EventLoop<'static, WindowState>,
}

impl WindowingSystem {
    fn new(config: &mut WindowConfig) -> Result<Self> {
        info!("Initializing WindowingSystem");
        let connection = Rc::new(Connection::connect_to_env()?);
        let event_queue = connection.new_event_queue();

        let (compositor, output, layer_shell, seat) =
            Self::initialize_globals(&connection, &event_queue.handle())?;

        let (surface, layer_surface) = Self::setup_surface(
            &compositor,
            &output,
            &layer_shell,
            &event_queue.handle(),
            config,
        );

        let pointer = Rc::new(seat.get_pointer(&event_queue.handle(), ()));
        let window = Self::initialize_renderer(&surface, &connection.display(), config)?;
        let component_definition = config
            .component_definition
            .take()
            .context("Component definition is required")?;

        let state = WindowStateBuilder::new()
            .with_component_definition(component_definition)
            .with_surface(Rc::clone(&surface))
            .with_layer_surface(Rc::clone(&layer_surface))
            .with_pointer(Rc::clone(&pointer))
            .with_scale_factor(config.scale_factor)
            .with_height(config.height)
            .with_exclusive_zone(config.exclusive_zone)
            .with_window(window)
            .build()?;

        let event_loop = EventLoop::try_new().context("Failed to create event loop")?;

        Ok(Self {
            state,
            connection,
            event_queue,
            event_loop,
        })
    }

    fn initialize_globals(
        connection: &Connection,
        queue_handle: &QueueHandle<WindowState>,
    ) -> Result<(WlCompositor, WlOutput, ZwlrLayerShellV1, WlSeat)> {
        let global_list = registry_queue_init::<WindowState>(connection)
            .map(|(global_list, _)| global_list)
            .context("Failed to initialize registry")?;

        let (compositor, output, layer_shell, seat) = bind_globals!(
            &global_list,
            queue_handle,
            (WlCompositor, compositor, 1..=1),
            (WlOutput, output, 1..=1),
            (ZwlrLayerShellV1, layer_shell, 1..=1),
            (WlSeat, seat, 1..=1)
        )?;

        Ok((compositor, output, layer_shell, seat))
    }

    fn setup_surface(
        compositor: &WlCompositor,
        output: &WlOutput,
        layer_shell: &ZwlrLayerShellV1,
        queue_handle: &QueueHandle<WindowState>,
        config: &WindowConfig,
    ) -> (Rc<WlSurface>, Rc<ZwlrLayerSurfaceV1>) {
        let surface = Rc::new(compositor.create_surface(queue_handle, ()));
        let layer_surface = Rc::new(layer_shell.get_layer_surface(
            &surface,
            Some(output),
            config.layer,
            config.namespace.clone(),
            queue_handle,
            (),
        ));

        Self::configure_layer_surface(&layer_surface, &surface, config);

        (surface, layer_surface)
    }

    fn configure_layer_surface(
        layer_surface: &Rc<ZwlrLayerSurfaceV1>,
        surface: &WlSurface,
        config: &WindowConfig,
    ) {
        layer_surface.set_anchor(config.anchor);
        layer_surface.set_margin(
            config.margin.0,
            config.margin.1,
            config.margin.2,
            config.margin.3,
        );

        layer_surface.set_exclusive_zone(config.exclusive_zone);
        layer_surface.set_keyboard_interactivity(config.keyboard_interactivity);
        layer_surface.set_size(1, config.height);
        surface.commit();
    }

    fn initialize_renderer(
        surface: &Rc<WlSurface>,
        display: &WlDisplay,
        config: &WindowConfig,
    ) -> Result<Rc<FemtoVGWindow>> {
        let init_size = PhysicalSize::new(1, 1);

        let context = EGLContext::builder()
            .with_display_id(display.id())
            .with_surface_id(surface.id())
            .with_size(init_size)
            .build()?;

        let renderer = FemtoVGRenderer::new(context).context("Failed to create FemtoVGRenderer")?;

        let femtovg_window = FemtoVGWindow::new(renderer);
        femtovg_window.set_size(slint::WindowSize::Physical(init_size));
        femtovg_window.set_scale_factor(config.scale_factor);
        femtovg_window.set_position(LogicalPosition::new(0., 0.));

        Ok(femtovg_window)
    }

    pub fn event_loop_handle(&self) -> LoopHandle<'static, WindowState> {
        self.event_loop.handle()
    }

    pub fn run(&mut self) -> Result<()> {
        info!("Starting WindowingSystem main loop");

        while self.event_queue.blocking_dispatch(&mut self.state)? > 0 {
            self.connection.flush()?;
            self.state.window().render_frame_if_dirty()?;
        }

        self.setup_wayland_event_source()?;

        let event_queue = &mut self.event_queue;
        let connection = &self.connection;

        self.event_loop
            .run(None, &mut self.state, move |shared_data| {
                if let Err(e) = Self::process_events(connection, event_queue, shared_data) {
                    error!("Error processing events: {}", e);
                }
            })
            .map_err(|e| anyhow::anyhow!("Failed to run event loop: {}", e))
    }

    fn setup_wayland_event_source(&self) -> Result<()> {
        debug!("Setting up Wayland event source");

        let connection = Rc::clone(&self.connection);

        self.event_loop
            .handle()
            .insert_source(
                calloop::generic::Generic::new(connection, Interest::READ, Mode::Level),
                move |_, _connection, _shared_data| Ok(PostAction::Continue),
            )
            .map_err(|e| anyhow::anyhow!("Failed to set up Wayland event source: {}", e))?;

        Ok(())
    }

    fn process_events(
        connection: &Connection,
        event_queue: &mut EventQueue<WindowState>,
        shared_data: &mut WindowState,
    ) -> Result<()> {
        if let Some(guard) = event_queue.prepare_read() {
            guard
                .read()
                .map_err(|e| anyhow::anyhow!("Failed to read events: {}", e))?;
        }
        connection.flush()?;

        event_queue.dispatch_pending(shared_data)?;

        slint::platform::update_timers_and_animations();

        shared_data.window().render_frame_if_dirty()?;

        Ok(())
    }

    pub const fn component_instance(&self) -> &ComponentInstance {
        self.state.component_instance()
    }

    pub fn window(&self) -> Rc<FemtoVGWindow> {
        self.state.window()
    }

    pub const fn state(&self) -> &WindowState {
        &self.state
    }
}
