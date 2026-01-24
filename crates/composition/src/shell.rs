use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

use layer_shika_adapters::errors::EventLoopError;
use layer_shika_adapters::platform::calloop::channel;
use layer_shika_adapters::platform::slint_interpreter::{
    CompilationResult, Compiler, ComponentInstance, Value,
};
use layer_shika_adapters::{
    AppState, ShellSurfaceConfig, SurfaceState, WaylandSurfaceConfig, WaylandSystemOps,
};
use layer_shika_domain::config::SurfaceConfig;
use layer_shika_domain::entities::output_registry::OutputRegistry;
use layer_shika_domain::errors::DomainError;
use layer_shika_domain::prelude::{
    AnchorEdges, KeyboardInteractivity, Layer, Margins, OutputPolicy, ScaleFactor, SurfaceDimension,
};
use layer_shika_domain::value_objects::handle::SurfaceHandle;
use layer_shika_domain::value_objects::output_handle::OutputHandle;
use layer_shika_domain::value_objects::output_info::OutputInfo;
use layer_shika_domain::value_objects::surface_instance_id::SurfaceInstanceId;
use spin_on::spin_on;

use crate::event_loop::{EventLoopHandle, FromAppState};
use crate::layer_surface::LayerSurfaceHandle;
use crate::session_lock::{SessionLock, SessionLockBuilder};
use crate::shell_config::{CompiledUiSource, ShellConfig};
use crate::shell_runtime::ShellRuntime;
use crate::surface_registry::{SurfaceDefinition, SurfaceEntry, SurfaceRegistry};
use crate::system::{
    CallbackContext, EventDispatchContext, PopupCommand, SessionLockCommand, ShellCommand,
    ShellControl, SurfaceCommand, SurfaceTarget,
};
use crate::value_conversion::IntoValue;
use crate::{Error, Result, logger};

/// Default Slint component name used when none is specified
pub const DEFAULT_COMPONENT_NAME: &str = "Main";

enum CompilationSource {
    File { path: PathBuf, compiler: Compiler },
    Source { code: String, compiler: Compiler },
    Compiled(Rc<CompilationResult>),
}

/// Builder for configuring and creating a Shell with one or more surfaces
///
/// Chain `.surface()` calls to configure multiple surfaces, then call `.build()` or `.run()`.
/// If no surfaces are configured, a default "Main" surface is created.
pub struct ShellBuilder {
    compilation: CompilationSource,
    surfaces: Vec<SurfaceDefinition>,
}

impl ShellBuilder {
    /// Starts configuration for a new surface with the given component name
    pub fn surface(self, component: impl Into<String>) -> SurfaceConfigBuilder {
        SurfaceConfigBuilder {
            shell_builder: self,
            component: component.into(),
            config: SurfaceConfig::default(),
        }
    }

    /// Discovers and registers multiple surfaces by component names
    #[must_use]
    pub fn discover_surfaces(
        mut self,
        components: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        for component in components {
            self.surfaces.push(SurfaceDefinition {
                component: component.into(),
                config: SurfaceConfig::default(),
            });
        }
        self
    }

    /// Builds the shell from the configured surfaces
    ///
    /// If no surfaces are configured, creates a minimal shell without layer surfaces.
    /// This is useful for lock-only applications that don't need persistent UI surfaces.
    pub fn build(self) -> Result<Shell> {
        let surfaces = self.surfaces;

        let compilation_result = match self.compilation {
            CompilationSource::File { path, compiler } => {
                let result = spin_on(compiler.build_from_path(&path));
                let diagnostics: Vec<_> = result.diagnostics().collect();
                if !diagnostics.is_empty() {
                    let messages: Vec<String> =
                        diagnostics.iter().map(ToString::to_string).collect();
                    return Err(DomainError::Configuration {
                        message: format!(
                            "Failed to compile Slint file '{}':\n{}",
                            path.display(),
                            messages.join("\n")
                        ),
                    }
                    .into());
                }
                Rc::new(result)
            }
            CompilationSource::Source { code, compiler } => {
                let result = spin_on(compiler.build_from_source(code, PathBuf::default()));
                let diagnostics: Vec<_> = result.diagnostics().collect();
                if !diagnostics.is_empty() {
                    let messages: Vec<String> =
                        diagnostics.iter().map(ToString::to_string).collect();
                    return Err(DomainError::Configuration {
                        message: format!(
                            "Failed to compile Slint source:\n{}",
                            messages.join("\n")
                        ),
                    }
                    .into());
                }
                Rc::new(result)
            }
            CompilationSource::Compiled(result) => result,
        };

        Shell::new(compilation_result, surfaces)
    }
}

/// Builder for configuring a single surface within a Shell
///
/// Chain configuration methods, then either start a new surface with `.surface()`
/// or finalize with `.build()` or `.run()`.
pub struct SurfaceConfigBuilder {
    shell_builder: ShellBuilder,
    component: String,
    config: SurfaceConfig,
}

impl SurfaceConfigBuilder {
    /// Sets both width and height for the surface
    #[must_use]
    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.config.dimensions = SurfaceDimension::new(width, height);
        self
    }

    /// Sets the height of the surface
    #[must_use]
    pub fn height(mut self, height: u32) -> Self {
        self.config.dimensions = SurfaceDimension::new(self.config.dimensions.width(), height);
        self
    }

    /// Sets the width of the surface
    #[must_use]
    pub fn width(mut self, width: u32) -> Self {
        self.config.dimensions = SurfaceDimension::new(width, self.config.dimensions.height());
        self
    }

    /// Sets the layer (stacking order) for the surface
    #[must_use]
    pub const fn layer(mut self, layer: Layer) -> Self {
        self.config.layer = layer;
        self
    }

    /// Sets the margins around the surface
    #[must_use]
    pub fn margin(mut self, margin: impl Into<Margins>) -> Self {
        self.config.margin = margin.into();
        self
    }

    /// Sets the anchor edges for positioning
    #[must_use]
    pub const fn anchor(mut self, anchor: AnchorEdges) -> Self {
        self.config.anchor = anchor;
        self
    }

    /// Sets the exclusive zone in pixels
    ///
    /// Reserves screen space that other windows avoid. Positive values reserve from anchored edge,
    /// `0` means no reservation, `-1` lets compositor decide.
    #[must_use]
    pub const fn exclusive_zone(mut self, zone: i32) -> Self {
        self.config.exclusive_zone = zone;
        self
    }

    /// Sets the namespace identifier for the surface
    #[must_use]
    pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
        self.config.namespace = namespace.into();
        self
    }

    /// Sets the scale factor for the surface
    #[must_use]
    pub fn scale_factor(mut self, sf: impl TryInto<ScaleFactor, Error = DomainError>) -> Self {
        self.config.scale_factor = sf.try_into().unwrap_or_default();
        self
    }

    /// Sets the keyboard interactivity mode
    #[must_use]
    pub const fn keyboard_interactivity(mut self, mode: KeyboardInteractivity) -> Self {
        self.config.keyboard_interactivity = mode;
        self
    }

    /// Sets the output policy for multi-monitor configuration
    ///
    /// Controls which monitors display this surface. Default is `OutputPolicy::All`.
    #[must_use]
    pub fn output_policy(mut self, policy: OutputPolicy) -> Self {
        self.config.output_policy = policy;
        self
    }

    /// Starts configuration for another surface
    #[must_use]
    pub fn surface(self, component: impl Into<String>) -> SurfaceConfigBuilder {
        let shell_builder = self.complete();
        shell_builder.surface(component)
    }

    /// Builds the shell with all configured surfaces
    pub fn build(self) -> Result<Shell> {
        self.complete().build()
    }

    /// Builds and runs the shell
    pub fn run(self) -> Result<()> {
        let mut shell = self.build()?;
        shell.run()
    }

    fn complete(mut self) -> ShellBuilder {
        self.shell_builder.surfaces.push(SurfaceDefinition {
            component: self.component,
            config: self.config,
        });
        self.shell_builder
    }
}

type OutputConnectedHandler = Box<dyn Fn(&OutputInfo)>;
type OutputDisconnectedHandler = Box<dyn Fn(OutputHandle)>;

/// Main runtime for managing Wayland layer-shell surfaces with Slint UI
///
/// Manages surface lifecycle, event loop integration, and component instantiation.
/// Supports multiple surfaces across monitors, dynamic spawning, and popup windows.
///
/// Create via `Shell::from_file()`, `from_source()`, or `from_compilation()`.
pub struct Shell {
    inner: Rc<RefCell<dyn WaylandSystemOps>>,
    registry: SurfaceRegistry,
    compilation_result: Rc<CompilationResult>,
    command_sender: channel::Sender<ShellCommand>,
    output_connected_handlers: Rc<RefCell<Vec<OutputConnectedHandler>>>,
    output_disconnected_handlers: Rc<RefCell<Vec<OutputDisconnectedHandler>>>,
}

impl Shell {
    /// Creates a shell builder from a Slint file path
    pub fn from_file(path: impl AsRef<Path>) -> ShellBuilder {
        ShellBuilder {
            compilation: CompilationSource::File {
                path: path.as_ref().to_path_buf(),
                compiler: Compiler::default(),
            },
            surfaces: Vec::new(),
        }
    }

    /// Creates a shell builder from a Slint file path with a custom compiler
    ///
    /// Useful for configuring include paths, style overrides, or compilation settings.
    pub fn from_file_with_compiler(path: impl AsRef<Path>, compiler: Compiler) -> ShellBuilder {
        ShellBuilder {
            compilation: CompilationSource::File {
                path: path.as_ref().to_path_buf(),
                compiler,
            },
            surfaces: Vec::new(),
        }
    }

    /// Creates a shell builder from Slint source code
    pub fn from_source(code: impl Into<String>) -> ShellBuilder {
        ShellBuilder {
            compilation: CompilationSource::Source {
                code: code.into(),
                compiler: Compiler::default(),
            },
            surfaces: Vec::new(),
        }
    }

    /// Creates a shell builder from Slint source code with a custom compiler
    pub fn from_source_with_compiler(code: impl Into<String>, compiler: Compiler) -> ShellBuilder {
        ShellBuilder {
            compilation: CompilationSource::Source {
                code: code.into(),
                compiler,
            },
            surfaces: Vec::new(),
        }
    }

    /// Creates a shell builder from a pre-compiled Slint compilation result
    pub fn from_compilation(result: Rc<CompilationResult>) -> ShellBuilder {
        ShellBuilder {
            compilation: CompilationSource::Compiled(result),
            surfaces: Vec::new(),
        }
    }

    /// Creates an empty shell builder for manual configuration
    pub fn builder() -> ShellBuilder {
        ShellBuilder {
            compilation: CompilationSource::Source {
                code: String::new(),
                compiler: Compiler::default(),
            },
            surfaces: Vec::new(),
        }
    }

    /// Compiles a Slint file and returns the compilation result
    pub fn compile_file(path: impl AsRef<Path>) -> Result<Rc<CompilationResult>> {
        let compiler = Compiler::default();
        let result = spin_on(compiler.build_from_path(path.as_ref()));
        let diagnostics: Vec<_> = result.diagnostics().collect();
        if !diagnostics.is_empty() {
            let messages: Vec<String> = diagnostics.iter().map(ToString::to_string).collect();
            return Err(DomainError::Configuration {
                message: format!(
                    "Failed to compile Slint file '{}':\n{}",
                    path.as_ref().display(),
                    messages.join("\n")
                ),
            }
            .into());
        }
        Ok(Rc::new(result))
    }

    /// Compiles Slint source code and returns the compilation result
    pub fn compile_source(code: impl Into<String>) -> Result<Rc<CompilationResult>> {
        let compiler = Compiler::default();
        let result = spin_on(compiler.build_from_source(code.into(), PathBuf::default()));
        let diagnostics: Vec<_> = result.diagnostics().collect();
        if !diagnostics.is_empty() {
            let messages: Vec<String> = diagnostics.iter().map(ToString::to_string).collect();
            return Err(DomainError::Configuration {
                message: format!("Failed to compile Slint source:\n{}", messages.join("\n")),
            }
            .into());
        }
        Ok(Rc::new(result))
    }

    /// Creates a shell from a complete configuration object
    pub fn from_config(config: ShellConfig) -> Result<Self> {
        let compilation_result = match config.ui_source {
            CompiledUiSource::File(path) => Self::compile_file(&path)?,
            CompiledUiSource::Source(code) => Self::compile_source(code)?,
            CompiledUiSource::Compiled(result) => result,
        };

        let surfaces: Vec<SurfaceDefinition> = if config.surfaces.is_empty() {
            vec![SurfaceDefinition {
                component: DEFAULT_COMPONENT_NAME.to_string(),
                config: SurfaceConfig::default(),
            }]
        } else {
            config
                .surfaces
                .into_iter()
                .map(|s| SurfaceDefinition {
                    component: s.component,
                    config: s.config,
                })
                .collect()
        };

        Self::new(compilation_result, surfaces)
    }

    pub(crate) fn new(
        compilation_result: Rc<CompilationResult>,
        definitions: Vec<SurfaceDefinition>,
    ) -> Result<Self> {
        logger::info!("Creating Shell with {} windows", definitions.len());

        for def in &definitions {
            def.config.validate().map_err(Error::Domain)?;
        }

        match definitions.len() {
            0 => {
                logger::info!("Creating minimal shell without layer surfaces");
                Self::new_minimal(compilation_result)
            }
            1 => {
                let definition = definitions.into_iter().next().ok_or_else(|| {
                    Error::Domain(DomainError::Configuration {
                        message: "Expected at least one window definition".to_string(),
                    })
                })?;
                Self::new_single_window(compilation_result, definition)
            }
            _ => Self::new_multi_window(compilation_result, &definitions),
        }
    }

    fn new_single_window(
        compilation_result: Rc<CompilationResult>,
        definition: SurfaceDefinition,
    ) -> Result<Self> {
        let component_definition = compilation_result
            .component(&definition.component)
            .ok_or_else(|| {
                Error::Domain(DomainError::ComponentNotFound {
                    name: definition.component.clone(),
                })
            })?;

        let handle = SurfaceHandle::new();
        let wayland_config = WaylandSurfaceConfig::from_domain_config(
            handle,
            &definition.component,
            component_definition,
            Some(Rc::clone(&compilation_result)),
            definition.config.clone(),
        );

        let inner = layer_shika_adapters::WaylandShellSystem::new(&wayland_config)?;
        let inner_rc: Rc<RefCell<dyn WaylandSystemOps>> = Rc::new(RefCell::new(inner));

        let (sender, receiver) = channel::channel();

        let mut registry = SurfaceRegistry::new();
        let entry = SurfaceEntry::new(handle, definition.component.clone(), definition);
        registry.insert(entry)?;

        let shell = Self {
            inner: Rc::clone(&inner_rc),
            registry,
            compilation_result,
            command_sender: sender,
            output_connected_handlers: Rc::new(RefCell::new(Vec::new())),
            output_disconnected_handlers: Rc::new(RefCell::new(Vec::new())),
        };

        shell.setup_command_handler(receiver)?;

        logger::info!("Shell created (single-window mode)");

        Ok(shell)
    }

    fn new_multi_window(
        compilation_result: Rc<CompilationResult>,
        definitions: &[SurfaceDefinition],
    ) -> Result<Self> {
        let shell_configs_with_handles: Vec<(SurfaceHandle, ShellSurfaceConfig)> = definitions
            .iter()
            .map(|def| {
                let component_definition = compilation_result
                    .component(&def.component)
                    .ok_or_else(|| {
                        Error::Domain(DomainError::ComponentNotFound {
                            name: def.component.clone(),
                        })
                    })?;

                let handle = SurfaceHandle::new();
                let wayland_config = WaylandSurfaceConfig::from_domain_config(
                    handle,
                    &def.component,
                    component_definition,
                    Some(Rc::clone(&compilation_result)),
                    def.config.clone(),
                );

                Ok((
                    handle,
                    ShellSurfaceConfig {
                        name: def.component.clone(),
                        config: wayland_config,
                    },
                ))
            })
            .collect::<Result<Vec<_>>>()?;

        let shell_configs: Vec<ShellSurfaceConfig> = shell_configs_with_handles
            .iter()
            .map(|(_, cfg)| cfg.clone())
            .collect();

        let inner = layer_shika_adapters::WaylandShellSystem::new_multi(&shell_configs)?;
        let inner_rc: Rc<RefCell<dyn WaylandSystemOps>> = Rc::new(RefCell::new(inner));

        let (sender, receiver) = channel::channel();

        let mut registry = SurfaceRegistry::new();
        for ((handle, _), definition) in shell_configs_with_handles.iter().zip(definitions.iter()) {
            let entry =
                SurfaceEntry::new(*handle, definition.component.clone(), definition.clone());
            registry.insert(entry)?;
        }

        let shell = Self {
            inner: Rc::clone(&inner_rc),
            registry,
            compilation_result,
            command_sender: sender,
            output_connected_handlers: Rc::new(RefCell::new(Vec::new())),
            output_disconnected_handlers: Rc::new(RefCell::new(Vec::new())),
        };

        shell.setup_command_handler(receiver)?;

        logger::info!(
            "Shell created (multi-surface mode) with surfaces: {:?}",
            shell.surface_names()
        );

        Ok(shell)
    }

    fn new_minimal(compilation_result: Rc<CompilationResult>) -> Result<Self> {
        let inner = layer_shika_adapters::WaylandShellSystem::new_minimal()?;
        let inner_rc: Rc<RefCell<dyn WaylandSystemOps>> = Rc::new(RefCell::new(inner));

        inner_rc
            .borrow_mut()
            .set_compilation_result(Rc::clone(&compilation_result));

        let (sender, receiver) = channel::channel();

        let registry = SurfaceRegistry::new();

        let shell = Self {
            inner: Rc::clone(&inner_rc),
            registry,
            compilation_result,
            command_sender: sender,
            output_connected_handlers: Rc::new(RefCell::new(Vec::new())),
            output_disconnected_handlers: Rc::new(RefCell::new(Vec::new())),
        };

        shell.setup_command_handler(receiver)?;

        logger::info!("Shell created (minimal mode - no layer surfaces)");

        Ok(shell)
    }

    fn setup_command_handler(&self, receiver: channel::Channel<ShellCommand>) -> Result<()> {
        let loop_handle = self.inner.borrow().event_loop_handle();
        let control = self.control();
        let system = Rc::downgrade(&self.inner);

        loop_handle
            .insert_source(receiver, move |event, (), app_state| {
                if let channel::Event::Msg(command) = event {
                    let mut ctx = crate::system::EventDispatchContext::from_app_state(app_state);

                    match command {
                        ShellCommand::Popup(popup_cmd) => {
                            Self::handle_popup_command(popup_cmd, &mut ctx, &control);
                        }
                        ShellCommand::Surface(surface_cmd) => {
                            Self::handle_surface_command(surface_cmd, &mut ctx);
                        }
                        ShellCommand::SessionLock(lock_cmd) => {
                            Self::handle_session_lock_command(&lock_cmd, &mut ctx, &system);
                        }
                        ShellCommand::Render => {
                            if let Err(e) = ctx.render_frame_if_dirty() {
                                logger::error!("Failed to render frame: {}", e);
                            }
                        }
                    }
                }
            })
            .map_err(|e| {
                Error::Adapter(
                    EventLoopError::InsertSource {
                        message: format!("Failed to setup command handler: {e:?}"),
                    }
                    .into(),
                )
            })?;

        Ok(())
    }

    fn handle_popup_command(
        command: PopupCommand,
        ctx: &mut EventDispatchContext<'_>,
        _control: &ShellControl,
    ) {
        match command {
            PopupCommand::Show { handle, config } => {
                if let Err(e) = ctx.show_popup(handle, &config) {
                    logger::error!("Failed to show popup: {}", e);
                }
            }
            PopupCommand::Close(handle) => {
                if let Err(e) = ctx.close_popup(handle) {
                    logger::error!("Failed to close popup: {}", e);
                }
            }
            PopupCommand::Resize {
                handle,
                width,
                height,
            } => {
                if let Err(e) = ctx.resize_popup(handle, width, height) {
                    logger::error!("Failed to resize popup: {}", e);
                }
            }
        }
    }

    fn handle_session_lock_command(
        command: &SessionLockCommand,
        ctx: &mut EventDispatchContext<'_>,
        _system: &Weak<RefCell<dyn WaylandSystemOps>>,
    ) {
        match command {
            SessionLockCommand::Activate {
                component_name,
                config,
            } => {
                logger::info!("Processing SessionLockCommand::Activate");
                if let Err(e) = ctx.activate_session_lock(component_name, config.clone()) {
                    logger::error!("Failed to activate session lock: {}", e);
                } else {
                    logger::info!("Session lock activated successfully");
                }
            }
            SessionLockCommand::Deactivate => {
                logger::info!("Processing SessionLockCommand::Deactivate");
                if let Err(e) = ctx.deactivate_session_lock() {
                    logger::error!("Failed to deactivate session lock: {}", e);
                } else {
                    logger::info!("Session lock deactivated successfully");
                }
            }
        }
    }

    fn resolve_surface_target<'a>(
        ctx: &'a mut EventDispatchContext<'_>,
        target: &SurfaceTarget,
    ) -> Vec<&'a mut SurfaceState> {
        match target {
            SurfaceTarget::ByInstance(id) => {
                if let Some(surface) = ctx.surface_by_instance_mut(id.surface(), id.output()) {
                    vec![surface]
                } else {
                    logger::warn!(
                        "Surface instance not found: handle {:?} on output {:?}",
                        id.surface(),
                        id.output()
                    );
                    vec![]
                }
            }
            SurfaceTarget::ByHandle(handle) => ctx.surfaces_by_handle_mut(*handle),
            SurfaceTarget::ByName(name) => ctx.surfaces_by_name_mut(name),
            SurfaceTarget::ByNameAndOutput { name, output } => {
                ctx.surfaces_by_name_and_output_mut(name, *output)
            }
        }
    }

    fn apply_surface_resize(
        ctx: &mut EventDispatchContext<'_>,
        target: &SurfaceTarget,
        width: u32,
        height: u32,
    ) {
        logger::debug!(
            "Surface command: Resize {:?} to {}x{}",
            target,
            width,
            height
        );
        for surface in Self::resolve_surface_target(ctx, target) {
            let handle = LayerSurfaceHandle::from_window_state(surface);
            handle.set_size(width, height);
            handle.commit();
            surface.update_size_with_compositor_logic(width, height);
        }
    }

    fn apply_surface_config_change<F>(
        ctx: &mut EventDispatchContext<'_>,
        target: &SurfaceTarget,
        operation: &str,
        apply: F,
    ) where
        F: Fn(&LayerSurfaceHandle<'_>),
    {
        logger::debug!("Surface command: {} {:?}", operation, target);
        for surface in Self::resolve_surface_target(ctx, target) {
            let handle = LayerSurfaceHandle::from_window_state(surface);
            apply(&handle);
            handle.commit();
        }
    }

    fn apply_full_config(
        ctx: &mut EventDispatchContext<'_>,
        target: &SurfaceTarget,
        config: &SurfaceConfig,
    ) {
        logger::debug!("Surface command: ApplyConfig {:?}", target);

        if let Err(e) = config.validate() {
            logger::error!("Invalid surface configuration: {}", e);
            return;
        }

        for surface in Self::resolve_surface_target(ctx, target) {
            let handle = LayerSurfaceHandle::from_window_state(surface);

            handle.set_size(config.dimensions.width(), config.dimensions.height());
            handle.set_anchor_edges(config.anchor);
            handle.set_exclusive_zone(config.exclusive_zone);
            handle.set_margins(config.margin);
            handle.set_layer(config.layer);
            handle.set_keyboard_interactivity(config.keyboard_interactivity);
            handle.commit();

            surface.update_size_with_compositor_logic(
                config.dimensions.width(),
                config.dimensions.height(),
            );
        }
    }

    fn handle_surface_command(command: SurfaceCommand, ctx: &mut EventDispatchContext<'_>) {
        match command {
            SurfaceCommand::Resize {
                target,
                width,
                height,
            } => {
                Self::apply_surface_resize(ctx, &target, width, height);
            }
            SurfaceCommand::SetAnchor { target, anchor } => {
                Self::apply_surface_config_change(ctx, &target, "SetAnchor", |handle| {
                    handle.set_anchor_edges(anchor);
                });
            }
            SurfaceCommand::SetExclusiveZone { target, zone } => {
                Self::apply_surface_config_change(ctx, &target, "SetExclusiveZone", |handle| {
                    handle.set_exclusive_zone(zone);
                });
            }
            SurfaceCommand::SetMargins { target, margins } => {
                Self::apply_surface_config_change(ctx, &target, "SetMargins", |handle| {
                    handle.set_margins(margins);
                });
            }
            SurfaceCommand::SetLayer { target, layer } => {
                Self::apply_surface_config_change(ctx, &target, "SetLayer", |handle| {
                    handle.set_layer(layer);
                });
            }
            SurfaceCommand::SetKeyboardInteractivity { target, mode } => {
                Self::apply_surface_config_change(
                    ctx,
                    &target,
                    "SetKeyboardInteractivity",
                    |handle| {
                        handle.set_keyboard_interactivity(mode);
                    },
                );
            }
            SurfaceCommand::SetOutputPolicy { target, policy } => {
                logger::debug!(
                    "Surface command: SetOutputPolicy {:?} to {:?}",
                    target,
                    policy
                );
                logger::warn!(
                    "SetOutputPolicy is not yet implemented - requires runtime surface spawning"
                );
            }
            SurfaceCommand::SetScaleFactor { target, factor } => {
                logger::debug!(
                    "Surface command: SetScaleFactor {:?} to {:?}",
                    target,
                    factor
                );
                logger::warn!(
                    "SetScaleFactor is not yet implemented - requires runtime surface property updates"
                );
            }
            SurfaceCommand::ApplyConfig { target, config } => {
                Self::apply_full_config(ctx, &target, &config);
            }
        }

        if let Err(e) = ctx.render_frame_if_dirty() {
            logger::error!("Failed to render frame after surface command: {}", e);
        }
    }

    /// Returns a control handle for sending commands to the shell
    #[must_use]
    pub fn control(&self) -> ShellControl {
        ShellControl::new(self.command_sender.clone())
    }

    /// Access popup management API
    #[must_use]
    pub fn popups(&self) -> crate::PopupShell {
        self.control().popups()
    }

    pub fn create_session_lock(&self, component: impl Into<String>) -> Result<SessionLock> {
        self.create_session_lock_with_config(SessionLockBuilder::new(component))
    }

    pub fn create_session_lock_with_config(
        &self,
        builder: SessionLockBuilder,
    ) -> Result<SessionLock> {
        let component = builder.component_name().to_string();
        if self.compilation_result.component(&component).is_none() {
            return Err(Error::Domain(DomainError::ComponentNotFound {
                name: component,
            }));
        }

        if !self.inner.borrow().is_session_lock_available() {
            return Err(Error::ProtocolNotAvailable {
                protocol: "ext-session-lock-v1".to_string(),
            });
        }

        Ok(builder.build(Rc::downgrade(&self.inner), self.command_sender.clone()))
    }

    /// Returns the names of all registered surfaces
    pub fn surface_names(&self) -> Vec<&str> {
        self.registry.surface_names()
    }

    /// Checks if a surface with the given name exists
    pub fn has_surface(&self, name: &str) -> bool {
        self.registry.contains_name(name)
    }

    /// Returns a handle to the event loop for registering custom event sources
    pub fn event_loop_handle(&self) -> EventLoopHandle {
        EventLoopHandle::new(Rc::downgrade(&self.inner))
    }

    /// Starts the event loop and runs the shell until exit
    pub fn run(&mut self) -> Result<()> {
        logger::info!(
            "Starting Shell event loop with {} windows",
            self.registry.len()
        );
        self.inner.borrow_mut().run()?;
        Ok(())
    }

    /// Spawns a new surface at runtime from the given definition
    ///
    /// The surface is instantiated on outputs according to its `OutputPolicy`.
    pub fn spawn_surface(&mut self, definition: SurfaceDefinition) -> Result<Vec<SurfaceHandle>> {
        let component_definition = self
            .compilation_result
            .component(&definition.component)
            .ok_or_else(|| {
                Error::Domain(DomainError::ComponentNotFound {
                    name: definition.component.clone(),
                })
            })?;

        let handle = SurfaceHandle::new();
        let wayland_config = WaylandSurfaceConfig::from_domain_config(
            handle,
            &definition.component,
            component_definition,
            Some(Rc::clone(&self.compilation_result)),
            definition.config.clone(),
        );

        let shell_config = ShellSurfaceConfig {
            name: definition.component.clone(),
            config: wayland_config,
        };

        let mut system = self.inner.borrow_mut();
        let handles = system.spawn_surface(&shell_config)?;

        let surface_handle = SurfaceHandle::new();
        let entry = SurfaceEntry::new(surface_handle, definition.component.clone(), definition);
        self.registry.insert(entry)?;

        logger::info!(
            "Spawned surface with handle {:?}, created {} output instances",
            surface_handle,
            handles.len()
        );

        Ok(vec![surface_handle])
    }

    /// Removes and destroys a surface by its handle
    pub fn despawn_surface(&mut self, handle: SurfaceHandle) -> Result<()> {
        let entry = self.registry.remove(handle).ok_or_else(|| {
            Error::Domain(DomainError::SurfaceNotFound {
                message: format!("Surface handle {:?} not found", handle),
            })
        })?;

        let mut system = self.inner.borrow_mut();
        system.despawn_surface(&entry.name)?;

        logger::info!(
            "Despawned surface '{}' with handle {:?}",
            entry.name,
            handle
        );

        Ok(())
    }

    /// Registers a handler called when a new output (monitor) is connected
    ///
    /// Surfaces with `OutputPolicy::All` spawn automatically on new outputs.
    pub fn on_output_connected<F>(&mut self, handler: F) -> Result<()>
    where
        F: Fn(&OutputInfo) + 'static,
    {
        self.output_connected_handlers
            .borrow_mut()
            .push(Box::new(handler));
        Ok(())
    }

    /// Registers a handler called when an output is disconnected
    pub fn on_output_disconnected<F>(&mut self, handler: F) -> Result<()>
    where
        F: Fn(OutputHandle) + 'static,
    {
        self.output_disconnected_handlers
            .borrow_mut()
            .push(Box::new(handler));
        Ok(())
    }

    /// Returns the handle for a surface by name
    pub fn get_surface_handle(&self, name: &str) -> Option<SurfaceHandle> {
        self.registry.handle_by_name(name)
    }

    /// Returns the name of a surface by its handle
    pub fn get_surface_name(&self, handle: SurfaceHandle) -> Option<&str> {
        self.registry.name_by_handle(handle)
    }

    /// Executes a function with access to a surface component instance by name
    pub fn with_surface<F, R>(&self, name: &str, f: F) -> Result<R>
    where
        F: FnOnce(&ComponentInstance) -> R,
    {
        if !self.registry.contains_name(name) {
            return Err(Error::Domain(DomainError::SurfaceNotFound {
                message: format!("Window '{}' not found", name),
            }));
        }

        let system = self.inner.borrow();

        system
            .app_state()
            .surfaces_by_name(name)
            .first()
            .map(|surface| f(surface.component_instance()))
            .ok_or_else(|| {
                Error::Domain(DomainError::SurfaceNotFound {
                    message: format!("No instance found for window '{}'", name),
                })
            })
    }

    /// Executes a function with each surface name and component instance
    pub fn with_all_surfaces<F>(&self, mut f: F)
    where
        F: FnMut(&str, &ComponentInstance),
    {
        let system = self.inner.borrow();

        for name in self.registry.surface_names() {
            for surface in system.app_state().surfaces_by_name(name) {
                f(name, surface.component_instance());
            }
        }
    }

    /// Executes a function with access to a surface on a specific output
    pub fn with_output<F, R>(&self, handle: OutputHandle, f: F) -> Result<R>
    where
        F: FnOnce(&ComponentInstance) -> R,
    {
        let system = self.inner.borrow();
        let window = system
            .app_state()
            .get_output_by_handle(handle)
            .ok_or_else(|| {
                Error::Domain(DomainError::OutputNotFound {
                    message: format!("Output with handle {:?} not found", handle),
                })
            })?;
        Ok(f(window.component_instance()))
    }

    /// Executes a function with each output handle and component instance
    pub fn with_all_outputs<F>(&self, mut f: F)
    where
        F: FnMut(OutputHandle, &ComponentInstance),
    {
        let system = self.inner.borrow();
        for (handle, surface) in system.app_state().outputs_with_handles() {
            f(handle, surface.component_instance());
        }
    }

    /// Returns the Slint compilation result used by this shell
    #[must_use]
    pub fn compilation_result(&self) -> &Rc<CompilationResult> {
        &self.compilation_result
    }

    /// Returns the registry of all connected outputs
    pub fn output_registry(&self) -> OutputRegistry {
        let system = self.inner.borrow();
        system.app_state().output_registry().clone()
    }

    /// Returns information about a specific output by handle
    pub fn get_output_info(&self, handle: OutputHandle) -> Option<OutputInfo> {
        let system = self.inner.borrow();
        system.app_state().get_output_info(handle).cloned()
    }

    /// Returns information about all connected outputs
    pub fn all_output_info(&self) -> Vec<OutputInfo> {
        let system = self.inner.borrow();
        system.app_state().all_output_info().cloned().collect()
    }

    /// Creates a selection for targeting specific surfaces by criteria
    pub fn select(&self, selector: impl Into<crate::Selector>) -> crate::Selection<'_> {
        crate::Selection::new(self, selector.into())
    }

    fn get_output_handles(&self) -> (Option<OutputHandle>, Option<OutputHandle>) {
        let registry = &self.output_registry();
        (registry.primary_handle(), registry.active_handle())
    }

    pub(crate) fn on_internal<F, R>(
        &self,
        selector: &crate::Selector,
        callback_name: &str,
        handler: F,
    ) where
        F: Fn(CallbackContext) -> R + Clone + 'static,
        R: IntoValue,
    {
        let control = self.control();
        let handler = Rc::new(handler);
        let system = self.inner.borrow();
        let (primary, active) = self.get_output_handles();

        for (key, surface) in system.app_state().surfaces_with_keys() {
            let surface_handle = key.surface_handle;
            let output_handle = key.output_handle;

            let surface_name = self.registry.by_handle(surface_handle).map_or_else(
                || format!("Unknown-{}", surface_handle.id()),
                |entry| entry.name.clone(),
            );

            let surface_info = crate::SurfaceInfo {
                name: surface_name.clone(),
                output: output_handle,
                instance_id: SurfaceInstanceId::new(surface_handle, output_handle),
            };

            let output_info = system.app_state().get_output_info(output_handle);

            if selector.matches(&surface_info, output_info, primary, active) {
                let instance_id = SurfaceInstanceId::new(surface_handle, output_handle);

                let handler_rc = Rc::clone(&handler);
                let control_clone = control.clone();
                let surface_name_clone = surface_name.clone();

                if let Err(e) =
                    surface
                        .component_instance()
                        .set_callback(callback_name, move |_args| {
                            let ctx = CallbackContext::new(
                                instance_id,
                                surface_name_clone.clone(),
                                control_clone.clone(),
                            );
                            handler_rc(ctx).into_value()
                        })
                {
                    logger::error!(
                        "Failed to register callback '{}' on surface '{}': {}",
                        callback_name,
                        surface_name,
                        e
                    );
                }
            }
        }
    }

    pub(crate) fn on_with_args_internal<F, R>(
        &self,
        selector: &crate::Selector,
        callback_name: &str,
        handler: F,
    ) where
        F: Fn(&[Value], CallbackContext) -> R + Clone + 'static,
        R: IntoValue,
    {
        let control = self.control();
        let handler = Rc::new(handler);
        let system = self.inner.borrow();
        let (primary, active) = self.get_output_handles();

        for (key, surface) in system.app_state().surfaces_with_keys() {
            let surface_handle = key.surface_handle;
            let output_handle = key.output_handle;

            let surface_name = self.registry.by_handle(surface_handle).map_or_else(
                || format!("Unknown-{}", surface_handle.id()),
                |entry| entry.name.clone(),
            );

            let surface_info = crate::SurfaceInfo {
                name: surface_name.clone(),
                output: output_handle,
                instance_id: SurfaceInstanceId::new(surface_handle, output_handle),
            };

            let output_info = system.app_state().get_output_info(output_handle);

            if selector.matches(&surface_info, output_info, primary, active) {
                let instance_id = SurfaceInstanceId::new(surface_handle, output_handle);

                let handler_rc = Rc::clone(&handler);
                let control_clone = control.clone();
                let surface_name_clone = surface_name.clone();

                if let Err(e) =
                    surface
                        .component_instance()
                        .set_callback(callback_name, move |args| {
                            let ctx = CallbackContext::new(
                                instance_id,
                                surface_name_clone.clone(),
                                control_clone.clone(),
                            );
                            handler_rc(args, ctx).into_value()
                        })
                {
                    logger::error!(
                        "Failed to register callback '{}' on surface '{}': {}",
                        callback_name,
                        surface_name,
                        e
                    );
                }
            }
        }
    }

    pub(crate) fn with_selected<F>(&self, selector: &crate::Selector, mut f: F)
    where
        F: FnMut(&str, &ComponentInstance),
    {
        let system = self.inner.borrow();
        let (primary, active) = self.get_output_handles();

        for (key, surface) in system.app_state().surfaces_with_keys() {
            let surface_name = system
                .app_state()
                .get_surface_name(key.surface_handle)
                .unwrap_or("unknown");
            let surface_info = crate::SurfaceInfo {
                name: surface_name.to_string(),
                output: key.output_handle,
                instance_id: crate::SurfaceInstanceId::new(key.surface_handle, key.output_handle),
            };

            let output_info = system.app_state().get_output_info(key.output_handle);

            if selector.matches(&surface_info, output_info, primary, active) {
                f(surface_name, surface.component_instance());
            }
        }
    }

    pub(crate) fn with_selected_info<F>(&self, selector: &crate::Selector, mut f: F)
    where
        F: FnMut(&crate::SurfaceInfo, &ComponentInstance),
    {
        let system = self.inner.borrow();
        let (primary, active) = self.get_output_handles();

        for (key, surface) in system.app_state().surfaces_with_keys() {
            let surface_name = system
                .app_state()
                .get_surface_name(key.surface_handle)
                .unwrap_or("unknown");
            let surface_info = crate::SurfaceInfo {
                name: surface_name.to_string(),
                output: key.output_handle,
                instance_id: crate::SurfaceInstanceId::new(key.surface_handle, key.output_handle),
            };

            let output_info = system.app_state().get_output_info(key.output_handle);

            if selector.matches(&surface_info, output_info, primary, active) {
                f(&surface_info, surface.component_instance());
            }
        }
    }

    pub(crate) fn configure_selected<F>(&self, selector: &crate::Selector, mut f: F)
    where
        F: FnMut(&ComponentInstance, LayerSurfaceHandle<'_>),
    {
        let system = self.inner.borrow();
        let (primary, active) = self.get_output_handles();

        for (key, surface) in system.app_state().surfaces_with_keys() {
            let surface_name = system
                .app_state()
                .get_surface_name(key.surface_handle)
                .unwrap_or("unknown");
            let surface_info = crate::SurfaceInfo {
                name: surface_name.to_string(),
                output: key.output_handle,
                instance_id: crate::SurfaceInstanceId::new(key.surface_handle, key.output_handle),
            };

            let output_info = system.app_state().get_output_info(key.output_handle);

            if selector.matches(&surface_info, output_info, primary, active) {
                let surface_handle = LayerSurfaceHandle::from_window_state(surface);
                f(surface.component_instance(), surface_handle);
            }
        }
    }

    pub(crate) fn count_selected(&self, selector: &crate::Selector) -> usize {
        let system = self.inner.borrow();
        let (primary, active) = self.get_output_handles();

        system
            .app_state()
            .surfaces_with_keys()
            .filter(|(key, _)| {
                let surface_name = system
                    .app_state()
                    .get_surface_name(key.surface_handle)
                    .unwrap_or("unknown");
                let surface_info = crate::SurfaceInfo {
                    name: surface_name.to_string(),
                    output: key.output_handle,
                    instance_id: crate::SurfaceInstanceId::new(
                        key.surface_handle,
                        key.output_handle,
                    ),
                };

                let output_info = system.app_state().get_output_info(key.output_handle);

                selector.matches(&surface_info, output_info, primary, active)
            })
            .count()
    }

    pub(crate) fn get_selected_info(&self, selector: &crate::Selector) -> Vec<crate::SurfaceInfo> {
        let system = self.inner.borrow();
        let (primary, active) = self.get_output_handles();

        system
            .app_state()
            .surfaces_with_keys()
            .filter_map(|(key, _)| {
                let surface_name = system
                    .app_state()
                    .get_surface_name(key.surface_handle)
                    .unwrap_or("unknown");
                let surface_info = crate::SurfaceInfo {
                    name: surface_name.to_string(),
                    output: key.output_handle,
                    instance_id: crate::SurfaceInstanceId::new(
                        key.surface_handle,
                        key.output_handle,
                    ),
                };

                let output_info = system.app_state().get_output_info(key.output_handle);

                if selector.matches(&surface_info, output_info, primary, active) {
                    Some(surface_info)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Creates a selection for targeting session lock surfaces by criteria
    pub fn select_lock(&self, selector: impl Into<crate::Selector>) -> crate::LockSelection<'_> {
        crate::LockSelection::new(self, selector.into())
    }

    fn selector_to_output_filter(selector: &crate::Selector) -> layer_shika_adapters::OutputFilter {
        let selector = selector.clone();

        Rc::new(
            move |component_name: &str,
                  output_handle: OutputHandle,
                  output_info: Option<&OutputInfo>,
                  primary_handle: Option<OutputHandle>,
                  active_handle: Option<OutputHandle>| {
                let surface_info = crate::SurfaceInfo {
                    name: component_name.to_string(),
                    output: output_handle,
                    instance_id: crate::SurfaceInstanceId::new(
                        crate::SurfaceHandle::from_raw(0),
                        output_handle,
                    ),
                };

                selector.matches(&surface_info, output_info, primary_handle, active_handle)
            },
        )
    }

    pub(crate) fn on_lock_internal<F, R>(
        &self,
        selector: &crate::Selector,
        callback_name: &str,
        handler: F,
    ) where
        F: Fn(CallbackContext) -> R + Clone + 'static,
        R: IntoValue,
    {
        let control = self.control();
        let handler = Rc::new(handler);
        let callback_name = callback_name.to_string();

        let callback_handler = Rc::new(move |_args: &[Value]| {
            let handler_rc = Rc::clone(&handler);
            let control_clone = control.clone();
            let instance_id =
                SurfaceInstanceId::new(SurfaceHandle::from_raw(0), OutputHandle::from_raw(0));
            let ctx = CallbackContext::new(instance_id, String::new(), control_clone);
            handler_rc(ctx).into_value()
        });

        let filter = Self::selector_to_output_filter(selector);
        self.inner
            .borrow_mut()
            .register_session_lock_callback_with_filter(&callback_name, callback_handler, filter);
    }

    pub(crate) fn on_lock_with_args_internal<F, R>(
        &self,
        selector: &crate::Selector,
        callback_name: &str,
        handler: F,
    ) where
        F: Fn(&[Value], CallbackContext) -> R + Clone + 'static,
        R: IntoValue,
    {
        let control = self.control();
        let handler = Rc::new(handler);
        let callback_name = callback_name.to_string();

        let callback_handler = Rc::new(move |args: &[Value]| {
            let handler_rc = Rc::clone(&handler);
            let control_clone = control.clone();
            let instance_id =
                SurfaceInstanceId::new(SurfaceHandle::from_raw(0), OutputHandle::from_raw(0));
            let ctx = CallbackContext::new(instance_id, String::new(), control_clone);
            handler_rc(args, ctx).into_value()
        });

        let filter = Self::selector_to_output_filter(selector);
        self.inner
            .borrow_mut()
            .register_session_lock_callback_with_filter(&callback_name, callback_handler, filter);
    }

    pub(crate) fn register_lock_property_internal(
        &self,
        selector: &crate::Selector,
        property_name: &str,
        value: Value,
    ) {
        use layer_shika_adapters::create_lock_property_operation_with_output_filter;

        let filter = Self::selector_to_output_filter(selector);
        let property_operation = create_lock_property_operation_with_output_filter(
            property_name,
            value,
            move |component_name, output_handle, output_info, primary, active| {
                filter(component_name, output_handle, output_info, primary, active)
            },
        );
        self.inner
            .borrow_mut()
            .register_session_lock_property_operation(property_operation);
    }

    pub(crate) fn with_selected_lock<F>(&self, selector: &crate::Selector, mut f: F)
    where
        F: FnMut(&str, &ComponentInstance),
    {
        let system = self.inner.borrow();
        let (primary, active) = self.get_output_handles();

        let Some(component_name) = system.session_lock_component_name() else {
            return;
        };

        system.iter_lock_surfaces(&mut |output_handle, component| {
            let surface_info = crate::SurfaceInfo {
                name: component_name.clone(),
                output: output_handle,
                instance_id: crate::SurfaceInstanceId::new(
                    crate::SurfaceHandle::from_raw(0),
                    output_handle,
                ),
            };

            let output_info = system.app_state().get_output_info(output_handle);

            if selector.matches(&surface_info, output_info, primary, active) {
                f(&component_name, component);
            }
        });
    }

    pub(crate) fn count_selected_lock(&self, selector: &crate::Selector) -> usize {
        let system = self.inner.borrow();
        let (primary, active) = self.get_output_handles();

        let Some(component_name) = system.session_lock_component_name() else {
            return 0;
        };

        let mut count = 0;
        system.iter_lock_surfaces(&mut |output_handle, _component| {
            let surface_info = crate::SurfaceInfo {
                name: component_name.clone(),
                output: output_handle,
                instance_id: crate::SurfaceInstanceId::new(
                    crate::SurfaceHandle::from_raw(0),
                    output_handle,
                ),
            };

            let output_info = system.app_state().get_output_info(output_handle);

            if selector.matches(&surface_info, output_info, primary, active) {
                count += 1;
            }
        });
        count
    }

    pub(crate) fn get_selected_lock_info(
        &self,
        selector: &crate::Selector,
    ) -> Vec<crate::SurfaceInfo> {
        let system = self.inner.borrow();
        let (primary, active) = self.get_output_handles();

        let Some(component_name) = system.session_lock_component_name() else {
            return Vec::new();
        };

        let mut info_vec = Vec::new();
        system.iter_lock_surfaces(&mut |output_handle, _component| {
            let surface_info = crate::SurfaceInfo {
                name: component_name.clone(),
                output: output_handle,
                instance_id: crate::SurfaceInstanceId::new(
                    crate::SurfaceHandle::from_raw(0),
                    output_handle,
                ),
            };

            let output_info = system.app_state().get_output_info(output_handle);

            if selector.matches(&surface_info, output_info, primary, active) {
                info_vec.push(surface_info);
            }
        });
        info_vec
    }
}

impl ShellRuntime for Shell {
    type LoopHandle = EventLoopHandle;
    type Context<'a> = ShellEventContext<'a>;

    fn event_loop_handle(&self) -> Self::LoopHandle {
        EventLoopHandle::new(Rc::downgrade(&self.inner))
    }

    fn with_component<F>(&self, name: &str, mut f: F)
    where
        F: FnMut(&ComponentInstance),
    {
        let system = self.inner.borrow();

        if self.registry.contains_name(name) {
            for surface in system.app_state().surfaces_by_name(name) {
                f(surface.component_instance());
            }
        }
    }

    fn with_all_components<F>(&self, mut f: F)
    where
        F: FnMut(&str, &ComponentInstance),
    {
        let system = self.inner.borrow();

        for name in self.registry.surface_names() {
            for surface in system.app_state().surfaces_by_name(name) {
                f(name, surface.component_instance());
            }
        }
    }

    fn run(&mut self) -> Result<()> {
        self.inner.borrow_mut().run()?;
        Ok(())
    }
}

/// Context providing access to shell state within custom event source callbacks
///
/// Obtained via event source callbacks registered through `EventLoopHandle`.
pub struct ShellEventContext<'a> {
    app_state: &'a mut AppState,
}

impl<'a> FromAppState<'a> for ShellEventContext<'a> {
    fn from_app_state(app_state: &'a mut AppState) -> Self {
        Self { app_state }
    }
}

impl ShellEventContext<'_> {
    /// Returns the component instance for a surface by name
    pub fn get_surface_component(&self, name: &str) -> Option<&ComponentInstance> {
        self.app_state
            .surfaces_by_name(name)
            .first()
            .map(|s| s.component_instance())
    }

    /// Returns all surface component instances
    pub fn all_surface_components(&self) -> impl Iterator<Item = &ComponentInstance> {
        self.app_state
            .all_outputs()
            .map(SurfaceState::component_instance)
    }

    /// Renders a new frame for all dirty surfaces
    pub fn render_frame_if_dirty(&mut self) -> Result<()> {
        for surface in self.app_state.all_outputs() {
            surface.render_frame_if_dirty()?;
        }
        Ok(())
    }

    /// Returns the primary output handle
    #[must_use]
    pub fn primary_output_handle(&self) -> Option<OutputHandle> {
        self.app_state.primary_output_handle()
    }

    /// Returns the active output handle
    #[must_use]
    pub fn active_output_handle(&self) -> Option<OutputHandle> {
        self.app_state.active_output_handle()
    }

    /// Returns the output registry
    pub fn output_registry(&self) -> &OutputRegistry {
        self.app_state.output_registry()
    }

    /// Returns all outputs with their handles and component instances
    pub fn outputs(&self) -> impl Iterator<Item = (OutputHandle, &ComponentInstance)> {
        self.app_state
            .outputs_with_handles()
            .map(|(handle, surface)| (handle, surface.component_instance()))
    }

    /// Returns the component instance for a specific output
    pub fn get_output_component(&self, handle: OutputHandle) -> Option<&ComponentInstance> {
        self.app_state
            .get_output_by_handle(handle)
            .map(SurfaceState::component_instance)
    }

    /// Returns information about a specific output
    pub fn get_output_info(&self, handle: OutputHandle) -> Option<&OutputInfo> {
        self.app_state.get_output_info(handle)
    }

    /// Returns information about all outputs
    pub fn all_output_info(&self) -> impl Iterator<Item = &OutputInfo> {
        self.app_state.all_output_info()
    }

    /// Returns all outputs with their info and component instances
    pub fn outputs_with_info(&self) -> impl Iterator<Item = (&OutputInfo, &ComponentInstance)> {
        self.app_state
            .outputs_with_info()
            .map(|(info, surface)| (info, surface.component_instance()))
    }

    /// Returns the compilation result if available
    #[must_use]
    pub fn compilation_result(&self) -> Option<Rc<CompilationResult>> {
        self.app_state
            .primary_output()
            .and_then(SurfaceState::compilation_result)
    }
}
