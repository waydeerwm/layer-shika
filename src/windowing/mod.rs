use self::state::WindowState;
use crate::{
    bind_globals,
    rendering::{egl_context::EGLContext, femtovg_window::FemtoVGWindow},
};
use anyhow::{Context, Result};
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
    globals::{registry_queue_init, GlobalList},
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
    fn new(config: &mut config::WindowConfig) -> Result<Self> {
        info!("Initializing WindowingSystem");
        let connection = Rc::new(Connection::connect_to_env()?);

        let global_list = Self::initialize_registry(&connection)?;
        let event_queue = connection.new_event_queue();

        let (compositor, output, layer_shell, seat) =
            Self::bind_globals(&global_list, &event_queue.handle())?;

        let surface = Rc::new(compositor.create_surface(&event_queue.handle(), ()));
        let layer_surface = Rc::new(layer_shell.get_layer_surface(
            &surface,
            Some(&output),
            config.layer,
            config.namespace.clone(),
            &event_queue.handle(),
            (),
        ));

        let pointer = Rc::new(seat.get_pointer(&event_queue.handle(), ()));

        Self::configure_layer_surface(&layer_surface, &surface, config);

        let mut state_builder = WindowStateBuilder::new()
            .component_definition(config.component_definition.take().unwrap())
            .surface(Rc::clone(&surface))
            .layer_surface(Rc::clone(&layer_surface))
            .pointer(Rc::clone(&pointer))
            .scale_factor(config.scale_factor)
            .height(config.height)
            .exclusive_zone(config.exclusive_zone);

        //Self::wait_for_configure(&mut event_queue, &mut state_builder)?;
        let display = connection.display();

        let window = Self::initialize_renderer(&state_builder, &display, config)?;
        state_builder = state_builder.window(window);

        let state = state_builder.build()?;

        let event_loop = EventLoop::try_new().context("Failed to create event loop")?;

        Ok(Self {
            state,
            connection,
            event_queue,
            event_loop,
        })
    }

    fn initialize_registry(connection: &Connection) -> Result<GlobalList> {
        registry_queue_init::<WindowState>(connection)
            .map(|(global_list, _)| global_list)
            .context("Failed to initialize registry")
    }

    fn bind_globals(
        global_list: &GlobalList,
        queue_handle: &QueueHandle<WindowState>,
    ) -> Result<(WlCompositor, WlOutput, ZwlrLayerShellV1, WlSeat)> {
        bind_globals!(
            global_list,
            queue_handle,
            (WlCompositor, compositor, 1..=1),
            (WlOutput, output, 1..=1),
            (ZwlrLayerShellV1, layer_shell, 1..=1),
            (WlSeat, seat, 1..=1)
        )
    }

    fn configure_layer_surface(
        layer_surface: &Rc<ZwlrLayerSurfaceV1>,
        surface: &WlSurface,
        config: &config::WindowConfig,
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

    fn create_renderer(
        state_builder: &WindowStateBuilder,
        display: &WlDisplay,
    ) -> Result<FemtoVGRenderer> {
        let size = state_builder.size.unwrap_or(PhysicalSize::new(1, 1));
        let surface = state_builder
            .surface
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Failed to get surface"))?;

        debug!("Creating EGL context with size: {:?}", size);
        let context = EGLContext::builder()
            .with_display_id(display.id())
            .with_surface_id(surface.id())
            .with_size(size)
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create EGL context: {:?}", e))?;

        debug!("Creating FemtoVGRenderer");
        FemtoVGRenderer::new(context).context("Failed to create FemtoVGRenderer")
    }

    fn initialize_renderer(
        state_builder: &WindowStateBuilder,
        display: &WlDisplay,
        config: &config::WindowConfig,
    ) -> Result<Rc<FemtoVGWindow>> {
        let renderer = Self::create_renderer(state_builder, display)?;

        let femtovg_window = FemtoVGWindow::new(renderer);
        let size = state_builder.size.unwrap_or_default();
        info!("Initializing UI with size: {:?}", size);
        femtovg_window.set_size(slint::WindowSize::Physical(size));
        femtovg_window.set_scale_factor(config.scale_factor);
        femtovg_window.set_position(LogicalPosition::new(0., 0.));

        debug!("Creating Slint component instance");

        Ok(femtovg_window)
    }

    pub fn event_loop_handle(&self) -> LoopHandle<'static, WindowState> {
        self.event_loop.handle()
    }

    pub fn run(&mut self) -> Result<()> {
        info!("Starting WindowingSystem main loop");

        self.state.window().render_frame_if_dirty()?;
        self.setup_wayland_event_source()?;

        let connection = Rc::clone(&self.connection);
        let event_queue = &mut self.event_queue;

        self.event_loop
            .run(None, &mut self.state, move |shared_data| {
                if let Err(e) =
                    Self::process_events(&Rc::clone(&connection), event_queue, shared_data)
                {
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
        connection: &Rc<Connection>,
        event_queue: &mut EventQueue<WindowState>,
        shared_data: &mut WindowState,
    ) -> Result<()> {
        connection.flush()?;

        if let Some(guard) = event_queue.prepare_read() {
            guard
                .read()
                .map_err(|e| anyhow::anyhow!("Failed to read events: {}", e))?;
        }

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
