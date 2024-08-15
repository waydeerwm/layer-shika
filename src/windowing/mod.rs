use self::{event_handler::WindowEventHandler, event_loop::EventLoopHandler, state::WindowState};
use crate::{
    bind_globals,
    rendering::{
        egl_context::EGLContext, femtovg_window::FemtoVGWindow, slint_platform::CustomSlintPlatform,
    },
};
use anyhow::{Context, Result};
use log::{debug, info};
use slint::{platform::femtovg_renderer::FemtoVGRenderer, ComponentHandle, LogicalPosition};
use slint_interpreter::{ComponentDefinition, ComponentInstance};
use smithay_client_toolkit::reexports::{
    calloop::{self, EventLoop},
    protocols_wlr::layer_shell::v1::client::{
        zwlr_layer_shell_v1::{self, ZwlrLayerShellV1},
        zwlr_layer_surface_v1::{Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1},
    },
};
use std::{
    cell::{Ref, RefCell},
    rc::Rc,
};
use wayland_client::{
    globals::{registry_queue_init, GlobalList},
    protocol::{
        wl_compositor::WlCompositor, wl_display::WlDisplay, wl_output::WlOutput, wl_seat::WlSeat,
        wl_surface::WlSurface,
    },
    Connection, EventQueue, Proxy, QueueHandle,
};

mod event_handler;
mod event_loop;
mod macros;
mod state;

pub struct WindowConfig {
    height: u32,
    layer: zwlr_layer_shell_v1::Layer,
    margin: (i32, i32, i32, i32),
    anchor: Anchor,
    keyboard_interactivity: KeyboardInteractivity,
    exclusive_zone: i32,
    scale_factor: f32,
    namespace: String,
    component_definition: Option<ComponentDefinition>,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            height: 30,
            layer: zwlr_layer_shell_v1::Layer::Top,
            margin: (0, 0, 0, 0),
            anchor: Anchor::Top | Anchor::Left | Anchor::Right,
            keyboard_interactivity: KeyboardInteractivity::OnDemand,
            exclusive_zone: -1,
            namespace: "layer-shika".to_owned(),
            scale_factor: 1.0,
            component_definition: None,
        }
    }
}

pub struct WindowingSystemBuilder {
    config: WindowConfig,
}

impl Default for WindowingSystemBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowingSystemBuilder {
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: WindowConfig::default(),
        }
    }

    #[must_use]
    pub const fn with_height(mut self, height: u32) -> Self {
        self.config.height = height;
        self
    }

    #[must_use]
    pub const fn with_layer(mut self, layer: zwlr_layer_shell_v1::Layer) -> Self {
        self.config.layer = layer;
        self
    }

    #[must_use]
    pub const fn with_margin(mut self, top: i32, right: i32, bottom: i32, left: i32) -> Self {
        self.config.margin = (top, right, bottom, left);
        self
    }

    #[must_use]
    pub const fn with_anchor(mut self, anchor: Anchor) -> Self {
        self.config.anchor = anchor;
        self
    }

    #[must_use]
    pub const fn with_keyboard_interactivity(
        mut self,
        interactivity: KeyboardInteractivity,
    ) -> Self {
        self.config.keyboard_interactivity = interactivity;
        self
    }

    #[must_use]
    pub const fn with_exclusive_zone(mut self, zone: i32) -> Self {
        self.config.exclusive_zone = zone;
        self
    }

    #[must_use]
    pub fn with_namespace(mut self, namespace: String) -> Self {
        self.config.namespace = namespace;
        self
    }

    #[must_use]
    pub const fn with_scale_factor(mut self, scale_factor: f32) -> Self {
        self.config.scale_factor = scale_factor;
        self
    }

    #[must_use]
    pub fn with_component_definition(mut self, component: ComponentDefinition) -> Self {
        self.config.component_definition = Some(component);
        self
    }

    pub fn build(self) -> Result<WindowingSystem<'static>> {
        if self.config.component_definition.is_none() {
            return Err(anyhow::anyhow!("Slint component not set"));
        }

        WindowingSystem::new(self.config)
    }
}

pub struct WindowingSystem<'a> {
    state: Rc<RefCell<WindowState>>,
    connection: Rc<Connection>,
    event_handler: Rc<RefCell<WindowEventHandler>>,
    window: Option<Rc<FemtoVGWindow>>,
    event_queue: Rc<RefCell<EventQueue<WindowEventHandler>>>,
    component_instance: Option<Rc<ComponentInstance>>,
    display: WlDisplay,
    config: WindowConfig,
    event_loop: EventLoop<'a, ()>,
    event_loop_handler: Option<EventLoopHandler>,
}

impl<'a> WindowingSystem<'a> {
    fn new(config: WindowConfig) -> Result<Self> {
        info!("Initializing WindowingSystem");
        let connection = Rc::new(Connection::connect_to_env()?);
        let state = Rc::new(RefCell::new(WindowState::new(&config)));
        let event_handler = Rc::new(RefCell::new(WindowEventHandler::new(Rc::downgrade(&state))));
        let display = connection.display();
        let event_queue = Rc::new(RefCell::new(connection.new_event_queue()));
        let global_list = Self::initialize_registry(&connection)?;
        let (compositor, output, layer_shell, seat) =
            Self::bind_globals(&global_list, &event_queue.borrow().handle())?;

        Self::setup_surface(
            &compositor,
            &output,
            &layer_shell,
            &seat,
            &event_queue.borrow().handle(),
            &event_handler,
            &config,
        );

        let event_loop = EventLoop::try_new().context("Failed to create event loop")?;

        let mut system = Self {
            state,
            connection,
            event_handler,
            window: None,
            event_queue,
            component_instance: None,
            display,
            config,
            event_loop,
            event_loop_handler: None,
        };

        system.wait_for_configure()?;
        system.initialize_renderer_and_ui()?;
        system.initialize_event_loop_handler();
        system.setup_event_sources()?;

        Ok(system)
    }

    fn initialize_registry(connection: &Connection) -> Result<GlobalList> {
        registry_queue_init::<WindowEventHandler>(connection)
            .map(|(global_list, _)| global_list)
            .context("Failed to initialize registry")
    }

    fn bind_globals(
        global_list: &GlobalList,
        queue_handle: &QueueHandle<WindowEventHandler>,
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
        queue_handle: &QueueHandle<WindowEventHandler>,
        event_handler: &Rc<RefCell<WindowEventHandler>>,
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

        let binding = event_handler.borrow_mut();
        let binding = binding.state();
        let mut state = binding.borrow_mut();
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

    fn wait_for_configure(&self) -> Result<()> {
        info!("Waiting for surface to be configured...");
        loop {
            self.connection.flush()?;
            self.event_queue
                .borrow_mut()
                .blocking_dispatch(&mut self.event_handler.borrow_mut())
                .context("Failed to dispatch events")?;

            let state = self.state.borrow();
            let size = state.output_size();
            if size.width > 1 && size.height > 1 {
                info!("Configured output size: {:?}", size);
                break;
            }
        }
        debug!("Surface configuration complete");
        Ok(())
    }

    fn initialize_renderer_and_ui(&mut self) -> Result<()> {
        let renderer = self.create_renderer()?;
        let component_definition = self
            .config
            .component_definition
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Component definition not set"))?;
        let (window, component_instance) =
            self.initialize_slint_ui(renderer, &component_definition)?;

        self.window = Some(Rc::clone(&window));
        self.state.borrow_mut().set_window(window);
        self.component_instance = Some(component_instance);

        Ok(())
    }

    fn create_renderer(&self) -> Result<FemtoVGRenderer> {
        let state_borrow = self.state.borrow();
        let size = state_borrow.size();
        let surface = state_borrow.surface().unwrap();

        debug!("Creating EGL context with size: {:?}", size);
        let context = EGLContext::builder()
            .with_display_id(self.display.id())
            .with_surface_id(surface.id())
            .with_size(size)
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create EGL context: {:?}", e))?;

        debug!("Creating FemtoVGRenderer");
        FemtoVGRenderer::new(context).context("Failed to create FemtoVGRenderer")
    }

    fn initialize_slint_ui(
        &self,
        renderer: FemtoVGRenderer,
        component_definition: &ComponentDefinition,
    ) -> Result<(Rc<FemtoVGWindow>, Rc<ComponentInstance>)> {
        let femtovg_window = FemtoVGWindow::new(renderer);
        let size = self.state.borrow().size();
        info!("Initializing UI with size: {:?}", size);
        femtovg_window.set_size(slint::WindowSize::Physical(size));
        femtovg_window.set_scale_factor(self.config.scale_factor);
        femtovg_window.set_position(LogicalPosition::new(0., 0.));

        debug!("Setting up custom Slint platform");
        let platform = CustomSlintPlatform::new(&femtovg_window);
        slint::platform::set_platform(Box::new(platform))
            .map_err(|e| anyhow::anyhow!("Failed to set platform: {:?}", e))?;

        debug!("Creating Slint component instance");
        let slint_component: Rc<ComponentInstance> = Rc::new(component_definition.create()?);

        slint_component
            .show()
            .map_err(|e| anyhow::anyhow!("Failed to show component: {:?}", e))?;

        Ok((femtovg_window, slint_component))
    }

    pub fn initialize_event_loop_handler(&mut self) {
        let event_loop_handler = EventLoopHandler::new(
            Rc::downgrade(self.window.as_ref().unwrap()),
            Rc::downgrade(&self.event_queue),
            Rc::downgrade(&self.connection),
            Rc::downgrade(&self.event_handler),
        );

        self.event_loop_handler = Some(event_loop_handler);
    }

    pub fn setup_event_sources(&self) -> Result<()> {
        let loop_handle = self.event_loop.handle();
        let event_loop_handler = self
            .event_loop_handler
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("EventLoopHandler not initialized"))?;

        event_loop_handler.setup_wayland_event_source(&loop_handle)?;

        Ok(())
    }

    pub fn event_loop_handle(&self) -> calloop::LoopHandle<'a, ()> {
        self.event_loop.handle()
    }

    pub fn run(&mut self) -> Result<()> {
        info!("Starting WindowingSystem main loop");
        if let Some(window) = &self.window {
            window.render_frame_if_dirty();
        }

        self.event_loop
            .run(None, &mut (), |()| {})
            .map_err(|e| anyhow::anyhow!("Failed to run event loop: {}", e))
    }

    pub fn component_instance(&self) -> Rc<ComponentInstance> {
        Rc::clone(self.component_instance.as_ref().unwrap())
    }

    pub fn window(&self) -> Rc<FemtoVGWindow> {
        Rc::clone(self.window.as_ref().unwrap())
    }

    pub fn state(&self) -> Ref<WindowState> {
        self.state.borrow()
    }

    pub const fn display(&self) -> &WlDisplay {
        &self.display
    }
}
