use crate::{logger, wayland::{
    config::{LayerSurfaceConfig, ShellSurfaceConfig, WaylandSurfaceConfig},
    globals::context::GlobalContext,
    managed_proxies::{ManagedWlKeyboard, ManagedWlPointer},
    ops::WaylandSystemOps,
    outputs::{OutputManager, OutputManagerContext},
    rendering::RenderableSet,
    session_lock::{LockPropertyOperation, OutputFilter},
    surfaces::{app_state::AppState, event_context::SharedPointerSerial, layer_surface::{SurfaceCtx, SurfaceSetupParams}, popup_manager::{PopupContext, PopupManager}, surface_builder::{PlatformWrapper, SurfaceStateBuilder}, surface_state::SurfaceState},
}};
use layer_shika_domain::value_objects::handle::SurfaceHandle;
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1;
use crate::{
    errors::{EventLoopError, LayerShikaError, RenderingError, Result},
    rendering::{
        egl::context_factory::RenderContextFactory,
        femtovg::{main_window::FemtoVGWindow, renderable_window::RenderableWindow},
        slint_integration::platform::CustomSlintPlatform,
    },
};
use core::result::Result as CoreResult;
use layer_shika_domain::errors::DomainError;
use layer_shika_domain::ports::shell::ShellSystemPort;
use layer_shika_domain::value_objects::lock_config::LockConfig;
use layer_shika_domain::value_objects::lock_state::LockState;
use layer_shika_domain::value_objects::output_handle::OutputHandle;
use layer_shika_domain::value_objects::output_info::OutputInfo;
use slint::{
    LogicalPosition, PhysicalSize, PlatformError, WindowPosition,
    platform::{WindowAdapter, femtovg_renderer::FemtoVGRenderer, set_platform, update_timers_and_animations},
};
use slint_interpreter::{ComponentInstance, CompilationResult};
use smithay_client_toolkit::reexports::calloop::{
    EventLoop, Interest, LoopHandle, Mode, PostAction, generic::Generic,
};
use std::cell::RefCell;
use std::rc::Rc;
use wayland_client::{
    Connection, EventQueue, Proxy, QueueHandle,
    backend::ObjectId,
    protocol::{wl_pointer::WlPointer, wl_surface::WlSurface},
};

type PopupManagersAndSurfaces = (Vec<Rc<PopupManager>>, Vec<Rc<ZwlrLayerSurfaceV1>>);

struct OutputSetup {
    output_id: ObjectId,
    main_surface_id: ObjectId,
    window: Rc<FemtoVGWindow>,
    builder: SurfaceStateBuilder,
    surface_handle: SurfaceHandle,
    shell_surface_name: String,
}

struct OutputManagerParams<'a> {
    config: &'a WaylandSurfaceConfig,
    global_ctx: &'a GlobalContext,
    connection: &'a Connection,
    layer_surface_config: LayerSurfaceConfig,
    render_factory: &'a Rc<RenderContextFactory>,
    popup_context: &'a PopupContext,
    pointer: &'a Rc<WlPointer>,
    shared_serial: &'a Rc<SharedPointerSerial>,
}

pub struct WaylandShellSystem {
    state: AppState,
    connection: Rc<Connection>,
    event_queue: EventQueue<AppState>,
    event_loop: EventLoop<'static, AppState>,
}

impl WaylandShellSystem {
    pub fn new(config: &WaylandSurfaceConfig) -> Result<Self> {
        logger::info!("Initializing WindowingSystem");
        let (connection, mut event_queue) = Self::init_wayland_connection()?;
        let event_loop =
            EventLoop::try_new().map_err(|e| EventLoopError::Creation { source: e })?;

        let state = Self::init_state(config, &connection, &mut event_queue)?;

        Ok(Self {
            state,
            connection,
            event_queue,
            event_loop,
        })
    }

    pub fn new_multi(configs: &[ShellSurfaceConfig]) -> Result<Self> {
        if configs.is_empty() {
            return Self::new_minimal();
        }

        logger::info!(
            "Initializing WindowingSystem with {} surface configs",
            configs.len()
        );
        let (connection, mut event_queue) = Self::init_wayland_connection()?;
        let event_loop =
            EventLoop::try_new().map_err(|e| EventLoopError::Creation { source: e })?;

        let state = Self::init_state_multi(configs, &connection, &mut event_queue)?;

        Ok(Self {
            state,
            connection,
            event_queue,
            event_loop,
        })
    }

    pub fn new_minimal() -> Result<Self> {
        logger::info!("Initializing WindowingSystem in minimal mode (no layer surfaces)");
        let (connection, mut event_queue) = Self::init_wayland_connection()?;
        let event_loop =
            EventLoop::try_new().map_err(|e| EventLoopError::Creation { source: e })?;

        let state = Self::init_state_minimal(&connection, &mut event_queue)?;

        Ok(Self {
            state,
            connection,
            event_queue,
            event_loop,
        })
    }

    fn init_wayland_connection() -> Result<(Rc<Connection>, EventQueue<AppState>)> {
        let connection = Rc::new(Connection::connect_to_env()?);
        let event_queue = connection.new_event_queue();
        Ok((connection, event_queue))
    }

    fn create_layer_surface_config(config: &WaylandSurfaceConfig) -> LayerSurfaceConfig {
        LayerSurfaceConfig {
            anchor: config.anchor,
            margin: config.margin,
            exclusive_zone: config.exclusive_zone,
            keyboard_interactivity: config.keyboard_interactivity,
            height: config.height,
            width: config.width,
        }
    }

    fn create_output_setups(
        config: &WaylandSurfaceConfig,
        global_ctx: &GlobalContext,
        connection: &Connection,
        event_queue: &mut EventQueue<AppState>,
        pointer: &Rc<WlPointer>,
        layer_surface_config: &LayerSurfaceConfig,
    ) -> Result<Vec<OutputSetup>> {
        let layer_shell =
            global_ctx
                .layer_shell
                .as_ref()
                .ok_or_else(|| LayerShikaError::InvalidInput {
                    message:
                        "wlr-layer-shell protocol not available - cannot create layer surfaces"
                            .into(),
                })?;

        let mut setups = Vec::new();

        for (index, output) in global_ctx.outputs.iter().enumerate() {
            let is_primary = index == 0;

            let mut temp_info = OutputInfo::new(OutputHandle::new());
            temp_info.set_primary(is_primary);

            if !config.output_policy.should_render(&temp_info) {
                logger::info!("Skipping output {} due to output policy", index);
                continue;
            }

            let output_id = output.id();

            let setup_params = SurfaceSetupParams {
                compositor: &global_ctx.compositor,
                output,
                layer_shell,
                fractional_scale_manager: global_ctx.fractional_scale_manager.as_ref(),
                viewporter: global_ctx.viewporter.as_ref(),
                queue_handle: &event_queue.handle(),
                layer: config.layer,
                namespace: config.namespace.clone(),
            };

            let surface_ctx = SurfaceCtx::setup(&setup_params, layer_surface_config);
            let main_surface_id = surface_ctx.surface.id();

            let render_factory =
                RenderContextFactory::new(Rc::clone(&global_ctx.render_context_manager));

            let window = Self::initialize_renderer(&surface_ctx.surface, config, &render_factory)?;

            let mut builder = SurfaceStateBuilder::new()
                .with_component_definition(config.component_definition.clone())
                .with_compilation_result(config.compilation_result.clone())
                .with_surface(Rc::clone(&surface_ctx.surface))
                .with_layer_surface(Rc::clone(&surface_ctx.layer_surface))
                .with_scale_factor(config.scale_factor)
                .with_height(config.height)
                .with_width(config.width)
                .with_exclusive_zone(config.exclusive_zone)
                .with_connection(Rc::new(connection.clone()))
                .with_pointer(Rc::clone(pointer))
                .with_window(Rc::clone(&window));

            if let Some(fs) = &surface_ctx.fractional_scale {
                builder = builder.with_fractional_scale(Rc::clone(fs));
            }

            if let Some(vp) = &surface_ctx.viewport {
                builder = builder.with_viewport(Rc::clone(vp));
            }

            setups.push(OutputSetup {
                output_id,
                main_surface_id,
                window,
                builder,
                surface_handle: config.surface_handle,
                shell_surface_name: config.surface_name.clone(),
            });
        }

        Ok(setups)
    }

    fn setup_platform(setups: &[OutputSetup]) -> Result<Rc<CustomSlintPlatform>> {
        let first_setup = setups
            .first()
            .ok_or_else(|| LayerShikaError::InvalidInput {
                message: "No outputs available".into(),
            })?;

        let platform = CustomSlintPlatform::new(&first_setup.window);

        for setup in setups.iter().skip(1) {
            platform.add_window(Rc::clone(&setup.window));
        }

        set_platform(Box::new(PlatformWrapper(Rc::clone(&platform))))
            .map_err(|e| LayerShikaError::PlatformSetup { source: e })?;

        Ok(platform)
    }

    fn create_window_states(
        setups: Vec<OutputSetup>,
        popup_context: &PopupContext,
        shared_serial: &Rc<SharedPointerSerial>,
        app_state: &mut AppState,
    ) -> Result<PopupManagersAndSurfaces> {
        let mut popup_managers = Vec::new();
        let mut layer_surfaces = Vec::new();

        for setup in setups {
            let mut per_output_surface = SurfaceState::new(setup.builder).map_err(|e| {
                LayerShikaError::WindowConfiguration {
                    message: e.to_string(),
                }
            })?;

            let popup_manager = Rc::new(PopupManager::new(
                popup_context.clone(),
                Rc::clone(per_output_surface.display_metrics()),
            ));

            per_output_surface.set_popup_manager(Rc::clone(&popup_manager));
            per_output_surface.set_shared_pointer_serial(Rc::clone(shared_serial));

            popup_managers.push(Rc::clone(&popup_manager));
            layer_surfaces.push(per_output_surface.layer_surface());

            app_state.add_shell_surface(
                &setup.output_id,
                setup.surface_handle,
                &setup.shell_surface_name,
                setup.main_surface_id,
                per_output_surface,
            );
        }

        Ok((popup_managers, layer_surfaces))
    }

    fn init_state(
        config: &WaylandSurfaceConfig,
        connection: &Connection,
        event_queue: &mut EventQueue<AppState>,
    ) -> Result<AppState> {
        let global_ctx = Rc::new(GlobalContext::initialize(
            connection,
            &event_queue.handle(),
        )?);
        let layer_surface_config = Self::create_layer_surface_config(config);

        let pointer = Rc::new(global_ctx.seat.get_pointer(&event_queue.handle(), ()));
        let keyboard = Rc::new(global_ctx.seat.get_keyboard(&event_queue.handle(), ()));
        let shared_serial = Rc::new(SharedPointerSerial::new());

        let mut app_state = AppState::new(
            ManagedWlPointer::new(Rc::clone(&pointer), Rc::new(connection.clone())),
            ManagedWlKeyboard::new(Rc::clone(&keyboard), Rc::new(connection.clone())),
            Rc::clone(&shared_serial),
        );

        app_state.set_queue_handle(event_queue.handle());

        let render_factory =
            RenderContextFactory::new(Rc::clone(&global_ctx.render_context_manager));

        let popup_context = PopupContext::new(
            global_ctx.compositor.clone(),
            global_ctx.xdg_wm_base.clone(),
            global_ctx.seat.clone(),
            global_ctx.fractional_scale_manager.clone(),
            global_ctx.viewporter.clone(),
            Rc::new(connection.clone()),
            Rc::clone(&render_factory),
        );

        let setups = Self::create_output_setups(
            config,
            global_ctx.as_ref(),
            connection,
            event_queue,
            &pointer,
            &layer_surface_config,
        )?;

        let platform = Self::setup_platform(&setups)?;
        app_state.set_slint_platform(Rc::clone(&platform));

        let (popup_managers, layer_surfaces) =
            Self::create_window_states(setups, &popup_context, &shared_serial, &mut app_state)?;

        Self::setup_shared_popup_creator(
            popup_managers,
            layer_surfaces,
            &platform,
            &event_queue.handle(),
            &shared_serial,
        );

        let output_manager = Self::create_output_manager(&OutputManagerParams {
            config,
            global_ctx: global_ctx.as_ref(),
            connection,
            layer_surface_config,
            render_factory: &render_factory,
            popup_context: &popup_context,
            pointer: &pointer,
            shared_serial: &shared_serial,
        });

        app_state.set_output_manager(Rc::new(RefCell::new(output_manager)));
        app_state.set_global_context(Rc::clone(&global_ctx));

        Ok(app_state)
    }

    fn init_state_multi(
        configs: &[ShellSurfaceConfig],
        connection: &Connection,
        event_queue: &mut EventQueue<AppState>,
    ) -> Result<AppState> {
        let global_ctx = Rc::new(GlobalContext::initialize(
            connection,
            &event_queue.handle(),
        )?);

        let pointer = Rc::new(global_ctx.seat.get_pointer(&event_queue.handle(), ()));
        let keyboard = Rc::new(global_ctx.seat.get_keyboard(&event_queue.handle(), ()));
        let shared_serial = Rc::new(SharedPointerSerial::new());

        let mut app_state = AppState::new(
            ManagedWlPointer::new(Rc::clone(&pointer), Rc::new(connection.clone())),
            ManagedWlKeyboard::new(Rc::clone(&keyboard), Rc::new(connection.clone())),
            Rc::clone(&shared_serial),
        );

        app_state.set_queue_handle(event_queue.handle());

        let render_factory =
            RenderContextFactory::new(Rc::clone(&global_ctx.render_context_manager));

        let popup_context = PopupContext::new(
            global_ctx.compositor.clone(),
            global_ctx.xdg_wm_base.clone(),
            global_ctx.seat.clone(),
            global_ctx.fractional_scale_manager.clone(),
            global_ctx.viewporter.clone(),
            Rc::new(connection.clone()),
            Rc::clone(&render_factory),
        );

        let setups = Self::create_output_setups_multi(
            configs,
            global_ctx.as_ref(),
            connection,
            event_queue,
            &pointer,
        )?;

        let platform = Self::setup_platform(&setups)?;
        app_state.set_slint_platform(Rc::clone(&platform));

        let (popup_managers, layer_surfaces) =
            Self::create_window_states(setups, &popup_context, &shared_serial, &mut app_state)?;

        Self::setup_shared_popup_creator(
            popup_managers,
            layer_surfaces,
            &platform,
            &event_queue.handle(),
            &shared_serial,
        );

        let primary_config = configs.first().map(|c| &c.config);
        if let Some(config) = primary_config {
            let layer_surface_config = Self::create_layer_surface_config(config);
            let output_manager = Self::create_output_manager(&OutputManagerParams {
                config,
                global_ctx: global_ctx.as_ref(),
                connection,
                layer_surface_config,
                render_factory: &render_factory,
                popup_context: &popup_context,
                pointer: &pointer,
                shared_serial: &shared_serial,
            });

            app_state.set_output_manager(Rc::new(RefCell::new(output_manager)));
        }

        app_state.set_global_context(Rc::clone(&global_ctx));

        Ok(app_state)
    }

    fn init_state_minimal(
        connection: &Connection,
        event_queue: &mut EventQueue<AppState>,
    ) -> Result<AppState> {
        let global_ctx = Rc::new(GlobalContext::initialize(
            connection,
            &event_queue.handle(),
        )?);

        let pointer = Rc::new(global_ctx.seat.get_pointer(&event_queue.handle(), ()));
        let keyboard = Rc::new(global_ctx.seat.get_keyboard(&event_queue.handle(), ()));
        let shared_serial = Rc::new(SharedPointerSerial::new());

        let mut app_state = AppState::new(
            ManagedWlPointer::new(Rc::clone(&pointer), Rc::new(connection.clone())),
            ManagedWlKeyboard::new(Rc::clone(&keyboard), Rc::new(connection.clone())),
            Rc::clone(&shared_serial),
        );

        app_state.set_queue_handle(event_queue.handle());
        app_state.set_global_context(Rc::clone(&global_ctx));

        let platform = CustomSlintPlatform::new_empty();
        set_platform(Box::new(PlatformWrapper(Rc::clone(&platform))))
            .map_err(|e| LayerShikaError::PlatformSetup { source: e })?;
        app_state.set_slint_platform(Rc::clone(&platform));

        logger::info!(
            "Minimal state initialized successfully (no layer surfaces, empty Slint platform for session locks)"
        );

        Ok(app_state)
    }

    fn create_output_setups_multi(
        configs: &[ShellSurfaceConfig],
        global_ctx: &GlobalContext,
        connection: &Connection,
        event_queue: &mut EventQueue<AppState>,
        pointer: &Rc<WlPointer>,
    ) -> Result<Vec<OutputSetup>> {
        let layer_shell =
            global_ctx
                .layer_shell
                .as_ref()
                .ok_or_else(|| LayerShikaError::InvalidInput {
                    message:
                        "wlr-layer-shell protocol not available - cannot create layer surfaces"
                            .into(),
                })?;

        let mut setups = Vec::new();

        for (output_index, output) in global_ctx.outputs.iter().enumerate() {
            let is_primary = output_index == 0;
            let output_id = output.id();

            for shell_config in configs {
                let config = &shell_config.config;

                let mut temp_info = OutputInfo::new(OutputHandle::new());
                temp_info.set_primary(is_primary);

                if !config.output_policy.should_render(&temp_info) {
                    logger::info!(
                        "Skipping shell surface '{}' on output {} due to output policy",
                        shell_config.name,
                        output_index
                    );
                    continue;
                }

                let layer_surface_config = Self::create_layer_surface_config(config);

                let setup_params = SurfaceSetupParams {
                    compositor: &global_ctx.compositor,
                    output,
                    layer_shell,
                    fractional_scale_manager: global_ctx.fractional_scale_manager.as_ref(),
                    viewporter: global_ctx.viewporter.as_ref(),
                    queue_handle: &event_queue.handle(),
                    layer: config.layer,
                    namespace: config.namespace.clone(),
                };

                let surface_ctx = SurfaceCtx::setup(&setup_params, &layer_surface_config);
                let main_surface_id = surface_ctx.surface.id();

                let render_factory =
                    RenderContextFactory::new(Rc::clone(&global_ctx.render_context_manager));

                let window =
                    Self::initialize_renderer(&surface_ctx.surface, config, &render_factory)?;

                let mut builder = SurfaceStateBuilder::new()
                    .with_component_definition(config.component_definition.clone())
                    .with_compilation_result(config.compilation_result.clone())
                    .with_surface(Rc::clone(&surface_ctx.surface))
                    .with_layer_surface(Rc::clone(&surface_ctx.layer_surface))
                    .with_scale_factor(config.scale_factor)
                    .with_height(config.height)
                    .with_width(config.width)
                    .with_exclusive_zone(config.exclusive_zone)
                    .with_connection(Rc::new(connection.clone()))
                    .with_pointer(Rc::clone(pointer))
                    .with_window(Rc::clone(&window));

                if let Some(fs) = &surface_ctx.fractional_scale {
                    builder = builder.with_fractional_scale(Rc::clone(fs));
                }

                if let Some(vp) = &surface_ctx.viewport {
                    builder = builder.with_viewport(Rc::clone(vp));
                }

                logger::info!(
                    "Created setup for shell surface '{}' on output {}",
                    shell_config.name,
                    output_index
                );

                setups.push(OutputSetup {
                    output_id: output_id.clone(),
                    main_surface_id,
                    window,
                    builder,
                    surface_handle: shell_config.config.surface_handle,
                    shell_surface_name: shell_config.name.clone(),
                });
            }
        }

        Ok(setups)
    }

    fn create_output_manager(params: &OutputManagerParams<'_>) -> OutputManager {
        let manager_context = OutputManagerContext {
            compositor: params.global_ctx.compositor.clone(),
            layer_shell: params.global_ctx.layer_shell.clone(),
            fractional_scale_manager: params.global_ctx.fractional_scale_manager.clone(),
            viewporter: params.global_ctx.viewporter.clone(),
            render_factory: Rc::clone(params.render_factory),
            popup_context: params.popup_context.clone(),
            pointer: Rc::clone(params.pointer),
            shared_serial: Rc::clone(params.shared_serial),
            connection: Rc::new(params.connection.clone()),
        };

        OutputManager::new(
            manager_context,
            params.config.clone(),
            params.layer_surface_config,
        )
    }

    fn setup_shared_popup_creator(
        popup_managers: Vec<Rc<PopupManager>>,
        layer_surfaces: Vec<Rc<ZwlrLayerSurfaceV1>>,
        platform: &Rc<CustomSlintPlatform>,
        queue_handle: &QueueHandle<AppState>,
        shared_serial: &Rc<SharedPointerSerial>,
    ) {
        let Some(first_manager) = popup_managers.first() else {
            logger::info!("No popup managers available");
            return;
        };

        if !first_manager.has_xdg_shell() {
            logger::info!("xdg-shell not available, popups will not be supported");
            return;
        }

        logger::info!(
            "Setting up shared popup creator for {} output(s)",
            popup_managers.len()
        );

        let queue_handle_clone = queue_handle.clone();
        let serial_holder = Rc::clone(shared_serial);

        platform.set_popup_creator(move || {
            logger::info!("Popup creator called! Searching for pending popup...");

            let serial = serial_holder.get();

            for (idx, (popup_manager, layer_surface)) in
                popup_managers.iter().zip(layer_surfaces.iter()).enumerate()
            {
                if popup_manager.has_pending_popup() {
                    logger::info!("Found pending popup in output #{}", idx);

                    let popup_surface = popup_manager
                        .create_pending_popup(&queue_handle_clone, layer_surface, serial)
                        .map_err(|e| {
                            PlatformError::Other(format!("Failed to create popup: {e}"))
                        })?;

                    logger::info!("Popup created successfully for output #{}", idx);
                    return Ok(popup_surface as Rc<dyn WindowAdapter>);
                }
            }

            Err(PlatformError::Other(
                "No pending popup request found in any output".into(),
            ))
        });
    }

    pub(crate) fn initialize_renderer(
        surface: &Rc<WlSurface>,
        config: &WaylandSurfaceConfig,
        render_factory: &Rc<RenderContextFactory>,
    ) -> Result<Rc<FemtoVGWindow>> {
        let init_size = PhysicalSize::new(1, 1);

        let context = render_factory.create_context(&surface.id(), init_size)?;

        let renderer = FemtoVGRenderer::new(context)
            .map_err(|e| LayerShikaError::FemtoVGRendererCreation { source: e })?;

        let femtovg_window = FemtoVGWindow::new(renderer);
        femtovg_window.set_size(slint::WindowSize::Physical(init_size));
        femtovg_window.set_scale_factor(config.scale_factor);
        femtovg_window.set_position(WindowPosition::Logical(LogicalPosition::new(0., 0.)));

        Ok(femtovg_window)
    }

    pub fn event_loop_handle(&self) -> LoopHandle<'static, AppState> {
        self.event_loop.handle()
    }

    pub fn run(&mut self) -> Result<()> {
        logger::info!("Starting WindowingSystem main loop");

        logger::info!("Processing initial Wayland configuration events");
        // first roundtrip to receive initial output, globals, and surface configure events
        // second roundtrip handles any cascading configure events like fractional scaling and layer surface configures
        for i in 0..2 {
            let dispatched = self.event_queue.roundtrip(&mut self.state)?;
            logger::info!("Roundtrip {} dispatched {} events", i + 1, dispatched);

            self.connection
                .flush()
                .map_err(|e| LayerShikaError::WaylandProtocol { source: e })?;

            update_timers_and_animations();

            self.state.render_all_dirty()?;
            if let Some(lock_manager) = self.state.lock_manager() {
                lock_manager.render_all_dirty()?;
            }
        }

        logger::info!("Initial configuration complete, requesting final render");
        for surface in self.state.all_outputs() {
            RenderableWindow::request_redraw(surface.window().as_ref());
        }
        update_timers_and_animations();
        self.state.render_all_dirty()?;
        if let Some(lock_manager) = self.state.lock_manager() {
            lock_manager.render_all_dirty()?;
        }
        self.connection
            .flush()
            .map_err(|e| LayerShikaError::WaylandProtocol { source: e })?;

        self.setup_wayland_event_source()?;

        let event_queue = &mut self.event_queue;
        let connection = &self.connection;

        self.event_loop
            .run(None, &mut self.state, move |shared_data| {
                if let Err(e) = Self::process_events(connection, event_queue, shared_data) {
                    logger::error!("Error processing events: {e}");
                }
            })
            .map_err(|e| EventLoopError::Execution { source: e })?;

        Ok(())
    }

    fn setup_wayland_event_source(&self) -> Result<()> {
        let connection = Rc::clone(&self.connection);

        self.event_loop
            .handle()
            .insert_source(
                Generic::new(connection, Interest::READ, Mode::Level),
                move |_, _connection, _shared_data| Ok(PostAction::Continue),
            )
            .map_err(|e| EventLoopError::InsertSource {
                message: format!("{e:?}"),
            })?;

        Ok(())
    }

    fn process_events(
        connection: &Connection,
        event_queue: &mut EventQueue<AppState>,
        shared_data: &mut AppState,
    ) -> Result<()> {
        if let Some(guard) = event_queue.prepare_read() {
            guard
                .read()
                .map_err(|e| LayerShikaError::WaylandProtocol { source: e })?;
        }

        event_queue.dispatch_pending(shared_data)?;

        update_timers_and_animations();

        if let Some(lock_manager) = shared_data.lock_manager_mut() {
            lock_manager.initialize_pending_components()?;
        }

        for surface in shared_data.all_outputs() {
            surface
                .window()
                .render_frame_if_dirty()
                .map_err(|e| RenderingError::Operation {
                    message: e.to_string(),
                })?;

            if let Some(popup_manager) = surface.popup_manager() {
                popup_manager
                    .render_popups()
                    .map_err(|e| RenderingError::Operation {
                        message: e.to_string(),
                    })?;
            }
        }

        if let Some(lock_manager) = shared_data.lock_manager() {
            lock_manager.render_all_dirty()?;
        }

        connection
            .flush()
            .map_err(|e| LayerShikaError::WaylandProtocol { source: e })?;

        Ok(())
    }

    pub fn component_instance(&self) -> Result<&ComponentInstance> {
        self.state
            .primary_output()
            .ok_or_else(|| LayerShikaError::InvalidInput {
                message: "No outputs available".into(),
            })
            .map(SurfaceState::component_instance)
    }

    pub fn state(&self) -> Result<&SurfaceState> {
        self.state
            .primary_output()
            .ok_or_else(|| LayerShikaError::InvalidInput {
                message: "No outputs available".into(),
            })
    }

    pub fn app_state(&self) -> &AppState {
        &self.state
    }

    pub fn app_state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    pub fn spawn_surface(&mut self, config: &ShellSurfaceConfig) -> Result<Vec<OutputHandle>> {
        logger::info!("Spawning new surface '{}'", config.name);

        let mut handles = Vec::new();

        for (output_handle, _surface) in self.state.outputs_with_handles() {
            handles.push(output_handle);
        }

        logger::info!(
            "Surface '{}' would spawn on {} outputs (dynamic spawning not yet fully implemented)",
            config.name,
            handles.len()
        );

        Ok(handles)
    }

    pub fn despawn_surface(&mut self, surface_name: &str) -> Result<()> {
        logger::info!("Despawning surface '{}'", surface_name);

        let removed = self.state.remove_surfaces_by_name(surface_name);

        logger::info!(
            "Removed {} surface instances for '{}'",
            removed.len(),
            surface_name
        );

        Ok(())
    }
}

impl ShellSystemPort for WaylandShellSystem {
    fn run(&mut self) -> CoreResult<(), DomainError> {
        WaylandShellSystem::run(self).map_err(|e| DomainError::Adapter {
            source: Box::new(e),
        })
    }
}

impl WaylandSystemOps for WaylandShellSystem {
    fn run(&mut self) -> Result<()> {
        WaylandShellSystem::run(self)
    }

    fn spawn_surface(&mut self, config: &ShellSurfaceConfig) -> Result<Vec<OutputHandle>> {
        WaylandShellSystem::spawn_surface(self, config)
    }

    fn despawn_surface(&mut self, name: &str) -> Result<()> {
        WaylandShellSystem::despawn_surface(self, name)
    }

    fn set_compilation_result(&mut self, compilation_result: Rc<CompilationResult>) {
        self.state.set_compilation_result(compilation_result);
    }

    fn activate_session_lock(&mut self, component_name: &str, config: LockConfig) -> Result<()> {
        self.state.activate_session_lock(component_name, config)
    }

    fn deactivate_session_lock(&mut self) -> Result<()> {
        self.state.deactivate_session_lock()
    }

    fn is_session_lock_available(&self) -> bool {
        self.state.is_session_lock_available()
    }

    fn session_lock_state(&self) -> Option<LockState> {
        self.state.current_lock_state()
    }

    fn register_session_lock_callback(
        &mut self,
        callback_name: &str,
        handler: Rc<dyn Fn(&[slint_interpreter::Value]) -> slint_interpreter::Value>,
    ) {
        self.state
            .register_session_lock_callback(callback_name, handler);
    }

    fn register_session_lock_callback_with_filter(
        &mut self,
        callback_name: &str,
        handler: Rc<dyn Fn(&[slint_interpreter::Value]) -> slint_interpreter::Value>,
        filter: OutputFilter,
    ) {
        self.state
            .register_session_lock_callback_with_filter(callback_name, handler, filter);
    }

    fn register_session_lock_property_operation(
        &mut self,
        property_operation: LockPropertyOperation,
    ) {
        self.state
            .register_session_lock_property_operation(property_operation);
    }

    fn session_lock_component_name(&self) -> Option<String> {
        self.state.session_lock_component_name()
    }

    fn iter_lock_surfaces(&self, f: &mut dyn FnMut(OutputHandle, &ComponentInstance)) {
        self.state.iter_lock_surfaces(f);
    }

    fn count_lock_surfaces(&self) -> usize {
        self.state.count_lock_surfaces()
    }

    fn app_state(&self) -> &AppState {
        WaylandShellSystem::app_state(self)
    }

    fn app_state_mut(&mut self) -> &mut AppState {
        WaylandShellSystem::app_state_mut(self)
    }

    fn event_loop_handle(&self) -> LoopHandle<'static, AppState> {
        WaylandShellSystem::event_loop_handle(self)
    }

    fn component_instance(&self) -> Result<&ComponentInstance> {
        WaylandShellSystem::component_instance(self)
    }
}
