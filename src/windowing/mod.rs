use self::state::WindowState;
use crate::{
    bind_globals,
    rendering::{
        egl_context::EGLContext, femtovg_window::FemtoVGWindow, slint_platform::CustomSlintPlatform,
    },
};
use anyhow::{Context, Result};
use config::WindowConfig;
use log::{debug, info};
use slint::{platform::femtovg_renderer::FemtoVGRenderer, ComponentHandle, LogicalPosition};
use slint_interpreter::ComponentInstance;
use smithay_client_toolkit::reexports::{
    calloop::{self, EventLoop, Interest, LoopHandle, Mode, PostAction},
    protocols_wlr::layer_shell::v1::client::{
        zwlr_layer_shell_v1::ZwlrLayerShellV1, zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    },
};
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
    fn new(config: &mut WindowConfig) -> Result<Self> {
        info!("Initializing WindowingSystem");
        let connection = Rc::new(Connection::connect_to_env()?);

        let global_list = Self::initialize_registry(&connection)?;
        let mut event_queue = connection.new_event_queue();

        let (compositor, output, layer_shell, seat) =
            Self::bind_globals(&global_list, &event_queue.handle())?;

        let mut state = WindowState::new(config);

        Self::setup_surface(
            &compositor,
            &output,
            &layer_shell,
            &seat,
            &event_queue.handle(),
            &mut state,
            config,
        );

        Self::wait_for_configure(&mut event_queue, &mut state)?;
        let display = connection.display();

        Self::initialize_renderer(&mut state, &display, config)?;

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

    fn setup_surface(
        compositor: &WlCompositor,
        output: &WlOutput,
        layer_shell: &ZwlrLayerShellV1,
        seat: &WlSeat,
        queue_handle: &QueueHandle<WindowState>,
        state: &mut WindowState,
        config: &WindowConfig,
    ) {
        let surface = Rc::new(compositor.create_surface(queue_handle, ()));
        let layer_surface = Rc::new(layer_shell.get_layer_surface(
            &surface,
            Some(output),
            config.layer,
            config.namespace.clone(),
            queue_handle,
            (),
        ));

        let pointer = Rc::new(seat.get_pointer(queue_handle, ()));

        state.set_surface(Rc::clone(&surface));
        state.set_layer_surface(Rc::clone(&layer_surface));
        state.set_pointer(pointer);

        Self::configure_layer_surface(&layer_surface, &surface, config);
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

    fn wait_for_configure(
        event_queue: &mut EventQueue<WindowState>,
        state: &mut WindowState,
    ) -> Result<()> {
        info!("Waiting for surface to be configured...");
        event_queue
            .blocking_dispatch(state)
            .context("Failed to dispatch events")?;
        info!("Blocking dispatch completed");
        let size = state.output_size();
        if size.width > 1 && size.height > 1 {
            info!("Configured output size: {:?}", size);
        } else {
            return Err(anyhow::anyhow!("Invalid output size: {:?}", size));
        }
        debug!("Surface configuration complete");
        Ok(())
    }

    fn create_renderer(state: &WindowState, display: &WlDisplay) -> Result<FemtoVGRenderer> {
        let size = state.size();
        let surface = state.surface().unwrap();

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
        state: &mut WindowState,
        display: &WlDisplay,
        config: &WindowConfig,
    ) -> Result<()> {
        let renderer = Self::create_renderer(state, display)?;

        let femtovg_window = FemtoVGWindow::new(renderer);
        let size = state.size();
        info!("Initializing UI with size: {:?}", size);
        femtovg_window.set_size(slint::WindowSize::Physical(size));
        femtovg_window.set_scale_factor(config.scale_factor);
        femtovg_window.set_position(LogicalPosition::new(0., 0.));

        //debug!("Setting up custom Slint platform");

        debug!("Creating Slint component instance");

        state.set_window(Rc::clone(&femtovg_window));

        Ok(())
    }

    pub fn event_loop_handle(&self) -> LoopHandle<'static, WindowState> {
        self.event_loop.handle()
    }

    pub fn run(&mut self) -> Result<()> {
        info!("Starting WindowingSystem main loop");
        if let Some(window) = &self.state.window() {
            /*let platform = CustomSlintPlatform::new(window);
            slint::platform::set_platform(Box::new(platform))
                .map_err(|e| anyhow::anyhow!("Failed to set platform: {:?}", e))?;
            */
            window.render_frame_if_dirty();
        }
        self.state.show_component();
        self.setup_wayland_event_source();

        self.event_loop
            .run(None, &mut self.state, |shared_data| {
                let _ = self.connection.flush();
                if let Some(guard) = self.event_queue.prepare_read() {
                    let _ = guard.read();
                }

                let _ = self.event_queue.dispatch_pending(shared_data);

                slint::platform::update_timers_and_animations();
                shared_data
                    .window()
                    .as_ref()
                    .unwrap()
                    .render_frame_if_dirty();
            })
            .map_err(|e| anyhow::anyhow!("Failed to run event loop: {}", e))
    }

    pub fn setup_wayland_event_source(&self) {
        debug!("Setting up Wayland event source");

        let connection = Rc::clone(&self.connection);

        let _ = self.event_loop.handle().insert_source(
            calloop::generic::Generic::new(connection, Interest::READ, Mode::Level),
            move |_, _connection, _shared_data| Ok(PostAction::Continue),
        );
    }

    pub fn component_instance(&self) -> &ComponentInstance {
        self.state.component_instance()
    }

    pub fn window(&self) -> Rc<FemtoVGWindow> {
        Rc::clone(self.state().window().as_ref().unwrap())
    }

    pub const fn state(&self) -> &WindowState {
        &self.state
    }
}
