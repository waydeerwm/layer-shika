use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use layer_shika_domain::prelude::KeyboardInteractivity;
use layer_shika_domain::value_objects::anchor::AnchorEdges;
use layer_shika_domain::value_objects::handle::SurfaceHandle;
use layer_shika_domain::value_objects::layer::Layer;
use layer_shika_domain::value_objects::margins::Margins;
use layer_shika_domain::value_objects::output_policy::OutputPolicy;
use slint::platform::{Platform, WindowAdapter, WindowProperties};
use slint::{
    LogicalPosition, LogicalSize, PhysicalSize, PlatformError, WindowPosition, WindowSize,
};
use ordermap::OrderMap;
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::Layer as WaylandLayer,
    zwlr_layer_surface_v1::{
        Anchor, KeyboardInteractivity as WaylandKeyboardInteractivity, ZwlrLayerSurfaceV1,
    },
};
use slint::platform::femtovg_renderer::FemtoVGRenderer;
use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::Proxy;

use crate::logger;
use crate::rendering::egl::context_factory::RenderContextFactory;
use crate::rendering::femtovg::main_window::FemtoVGWindow;
use crate::rendering::femtovg::renderable_window::RenderableWindow;
use crate::wayland::config::LayerSurfaceConfig;
use crate::wayland::globals::context::GlobalContext;
use crate::wayland::surfaces::app_state::AppState;
use crate::wayland::surfaces::layer_surface::{SurfaceCtx, SurfaceSetupParams};
use crate::wayland::surfaces::surface_builder::SurfaceStateBuilder;
use crate::wayland::surfaces::surface_state::SurfaceState;
use crate::wayland::shell_adapter::WaylandShellSystem;

const DEFAULT_SIZE: u32 = 50;

#[derive(Clone)]
struct LayerShellConfig {
    anchor: AnchorEdges,
    margin: Margins,
    exclusive_zone: Option<i32>,
    keyboard_interactivity: KeyboardInteractivity,
    height: Option<u32>,
    width: Option<u32>,
    last_preferred: Option<LogicalSize>,
    layer: Layer,
    scale_factor: f32,
}

impl Default for LayerShellConfig {
    fn default() -> Self {
        Self {
            anchor: AnchorEdges::default(),
            margin: Margins::default(),
            exclusive_zone: None,
            keyboard_interactivity: KeyboardInteractivity::None,
            height: None,
            width: None,
            last_preferred: None,
            layer: Layer::Top,
            scale_factor: 1.0,
        }
    }
}

impl LayerShellConfig {
    fn wayland_anchor(&self) -> Anchor {
        let mut result = Anchor::empty();

        if self.anchor.has_top() {
            result = result.union(Anchor::Top);
        }
        if self.anchor.has_bottom() {
            result = result.union(Anchor::Bottom);
        }
        if self.anchor.has_left() {
            result = result.union(Anchor::Left);
        }
        if self.anchor.has_right() {
            result = result.union(Anchor::Right);
        }

        result
    }

    fn wayland_layer(&self) -> WaylandLayer {
        match self.layer {
            Layer::Background => WaylandLayer::Background,
            Layer::Bottom => WaylandLayer::Bottom,
            Layer::Top => WaylandLayer::Top,
            Layer::Overlay => WaylandLayer::Overlay,
        }
    }

    fn wayland_keyboard_interactivity(&self) -> WaylandKeyboardInteractivity {
        match self.keyboard_interactivity {
            KeyboardInteractivity::None => WaylandKeyboardInteractivity::None,
            KeyboardInteractivity::Exclusive => WaylandKeyboardInteractivity::Exclusive,
            KeyboardInteractivity::OnDemand => WaylandKeyboardInteractivity::OnDemand,
        }
    }

    fn layer_surface_config(&self) -> LayerSurfaceConfig {
        let (width, height) = self.resolve_size();
        LayerSurfaceConfig {
            anchor: self.wayland_anchor(),
            margin: self.margin,
            exclusive_zone: self.resolve_exclusive_zone(width, height),
            keyboard_interactivity: self.wayland_keyboard_interactivity(),
            height,
            width,
        }
    }

    fn apply_to(&self, entry: &LayerShellSurfaceEntry) {
        let (width, height) = self.resolve_size();
        entry.layer_surface.set_anchor(self.wayland_anchor());
        entry.layer_surface.set_margin(
            self.margin.top,
            self.margin.right,
            self.margin.bottom,
            self.margin.left,
        );
        entry
            .layer_surface
            .set_exclusive_zone(self.resolve_exclusive_zone(width, height));
        entry
            .layer_surface
            .set_keyboard_interactivity(self.wayland_keyboard_interactivity());
        entry.layer_surface.set_size(width, height);
        entry.layer_surface.set_layer(self.wayland_layer());
        entry.surface.commit();
        entry.window.set_scale_factor(self.scale_factor);
    }

    fn resolve_size(&self) -> (u32, u32) {
        let width = if let Some(width) = self.width {
            width
        } else if self.anchor.has_left() && self.anchor.has_right() {
            0
        } else if let Some(pref) = self.last_preferred {
            pref.width.max(0.0).round() as u32
        } else {
            DEFAULT_SIZE
        };

        let height = if let Some(height) = self.height {
            height
        } else if self.anchor.has_top() && self.anchor.has_bottom() {
            0
        } else if let Some(pref) = self.last_preferred {
            pref.height.max(0.0).round() as u32
        } else {
            DEFAULT_SIZE
        };

        (width, height)
    }

    fn resolve_exclusive_zone(&self, width: u32, height: u32) -> i32 {
        if let Some(zone) = self.exclusive_zone {
            return zone;
        }

        if self.anchor.has_top() ^ self.anchor.has_bottom() {
            height as i32
        } else if self.anchor.has_left() ^ self.anchor.has_right() {
            width as i32
        } else {
            0
        }
    }
}

struct LayerShellSurfaceEntry {
    surface_id: ObjectId,
    surface: Rc<WlSurface>,
    layer_surface: Rc<ZwlrLayerSurfaceV1>,
    window: Rc<FemtoVGWindow>,
}

struct LayerShellConfigState {
    configs_by_title: HashMap<String, LayerShellConfig>,
    surfaces_by_id: HashMap<ObjectId, LayerShellSurfaceEntry>,
    surface_id_by_title: HashMap<String, ObjectId>,
    title_by_surface_id: HashMap<ObjectId, String>,
}

impl LayerShellConfigState {
    fn new() -> Self {
        Self {
            configs_by_title: HashMap::new(),
            surfaces_by_id: HashMap::new(),
            surface_id_by_title: HashMap::new(),
            title_by_surface_id: HashMap::new(),
        }
    }

    fn register_surface(&mut self, entry: LayerShellSurfaceEntry) {
        self.surfaces_by_id.insert(entry.surface_id.clone(), entry);
    }

    fn register_title(&mut self, surface_id: &ObjectId, title: &str) {
        if let Some(old_title) = self.title_by_surface_id.get(surface_id).cloned() {
            if old_title != title {
                self.surface_id_by_title.remove(&old_title);
            }
        }

        self.surface_id_by_title
            .insert(title.to_string(), surface_id.clone());
        self.title_by_surface_id
            .insert(surface_id.clone(), title.to_string());

        if let Some(entry) = self.surfaces_by_id.get(surface_id) {
            if let Some(config) = self.configs_by_title.get(title) {
                config.apply_to(entry);
            }
        }
    }

    fn update_preferred_size(&mut self, title: &str, preferred: LogicalSize) {
        let config = self.configs_by_title.entry(title.to_string()).or_default();
        config.last_preferred = Some(preferred);

        if let Some(surface_id) = self.surface_id_by_title.get(title) {
            if let Some(entry) = self.surfaces_by_id.get(surface_id) {
                config.apply_to(entry);
            }
        }
    }

    fn set_for_title<F>(&mut self, title: &str, updater: F)
    where
        F: FnOnce(&mut LayerShellConfig),
    {
        let config = self.configs_by_title.entry(title.to_string()).or_default();
        updater(config);

        if let Some(surface_id) = self.surface_id_by_title.get(title) {
            if let Some(entry) = self.surfaces_by_id.get(surface_id) {
                config.apply_to(entry);
            }
        }
    }
}

pub struct LayerShell {
    system: RefCell<Option<WaylandShellSystem>>,
    pending_windows: RefCell<Vec<Rc<dyn WindowAdapter>>>,
    config_state: Rc<RefCell<LayerShellConfigState>>,
    namespaces: Rc<RefCell<OrderMap<String, String>>>,
    output_policies: Rc<RefCell<OrderMap<String, OutputPolicy>>>,
}

impl LayerShell {
    pub fn new() -> Result<Box<Self>, PlatformError> {
        Ok(Box::new(Self {
            system: RefCell::new(None),
            pending_windows: RefCell::new(Vec::new()),
            config_state: Rc::new(RefCell::new(LayerShellConfigState::new())),
            namespaces: Rc::new(RefCell::new(OrderMap::new())),
            output_policies: Rc::new(RefCell::new(OrderMap::new())),
        }))
    }

    pub fn window(&self, title: impl Into<String>) -> LayerShellWindow {
        LayerShellWindow {
            title: title.into(),
            config_state: Rc::clone(&self.config_state),
            namespaces: Rc::clone(&self.namespaces),
            output_policies: Rc::clone(&self.output_policies),
        }
    }

    fn ensure_system(&self) -> Result<(), PlatformError> {
        if self.system.borrow().is_some() {
            return Ok(());
        }

        let system = WaylandShellSystem::new_layer_shell()
            .map_err(|e| PlatformError::Other(format!("Wayland init failed: {e}")))?;
        *self.system.borrow_mut() = Some(system);
        Ok(())
    }

    fn select_output<'a>(
        global_ctx: &'a GlobalContext,
        app_state: &mut AppState,
        policy: &OutputPolicy,
    ) -> Result<&'a WlOutput, PlatformError> {
        if global_ctx.outputs.is_empty() {
            return Err(PlatformError::Other("No outputs available".to_string()));
        }

        for output in &global_ctx.outputs {
            let handle = app_state.ensure_output_registered(&output.id());
            if let Some(info) = app_state.get_output_info(handle) {
                if policy.should_render(info) {
                    return Ok(output);
                }
            }
        }

        Err(PlatformError::Other(
            "No outputs matched output policy".to_string(),
        ))
    }
}

impl Platform for LayerShell {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        self.ensure_system()?;
        let mut system_ref = self.system.borrow_mut();
        let system = system_ref
            .as_mut()
            .ok_or_else(|| PlatformError::Other("Wayland system unavailable".to_string()))?;

        let global_ctx = system
            .global_context()
            .ok_or_else(|| PlatformError::Other("Wayland globals unavailable".to_string()))?;
        let render_factory =
            RenderContextFactory::new(Rc::clone(&global_ctx.render_context_manager));
        let pointer = Rc::new(
            global_ctx
                .seat
                .get_pointer(&system.event_queue_handle(), ()),
        );
        let shared_serial = system
            .shared_serial()
            .ok_or_else(|| PlatformError::Other("Wayland serial unavailable".to_string()))?;

        let layer_shell = global_ctx.layer_shell.as_ref().ok_or_else(|| {
            PlatformError::Other(
                "wlr-layer-shell protocol not available - cannot create layer surfaces".to_string(),
            )
        })?;

        let output_policy = self
            .output_policies
            .borrow_mut()
            .remove_index(0)
            .map_or_else(|| OutputPolicy::PrimaryOnly, |(_, policy)| policy);

        let output =
            LayerShell::select_output(&global_ctx, system.app_state_mut(), &output_policy)?;

        let config = LayerShellConfig::default();
        let namespace = self
            .namespaces
            .borrow_mut()
            .remove_index(0)
            .map_or_else(|| "layer-shika".into(), |(_, n)| n);

        let queue_handle = system.event_queue_handle();

        let setup_params = SurfaceSetupParams {
            compositor: &global_ctx.compositor,
            output,
            layer_shell,
            fractional_scale_manager: global_ctx.fractional_scale_manager.as_ref(),
            viewporter: global_ctx.viewporter.as_ref(),
            queue_handle: &queue_handle,
            layer: config.wayland_layer(),
            namespace,
        };

        let surface_ctx = SurfaceCtx::setup(&setup_params, &config.layer_surface_config());
        let main_surface_id = surface_ctx.surface.id();

        let init_size = PhysicalSize::new(1, 1);
        let context = render_factory
            .create_context(&surface_ctx.surface.id(), init_size)
            .map_err(|e| PlatformError::Other(format!("Renderer context failed: {e}")))?;
        let renderer = FemtoVGRenderer::new(context)
            .map_err(|e| PlatformError::Other(format!("Renderer init failed: {e}")))?;
        let window = FemtoVGWindow::new(renderer);
        window.set_size(WindowSize::Physical(init_size));
        window.set_scale_factor(config.scale_factor);
        window.set_position(WindowPosition::Logical(LogicalPosition::new(0., 0.)));

        let (width, height) = config.resolve_size();
        let exclusive_zone = config.resolve_exclusive_zone(width, height);
        let connection = system.connection();
        let mut builder = SurfaceStateBuilder::new()
            .with_surface(Rc::clone(&surface_ctx.surface))
            .with_layer_surface(Rc::clone(&surface_ctx.layer_surface))
            .with_scale_factor(config.scale_factor)
            .with_height(height)
            .with_width(width)
            .with_exclusive_zone(exclusive_zone)
            .with_connection(Rc::clone(&connection))
            .with_pointer(Rc::clone(&pointer))
            .with_window(Rc::clone(&window));

        if let Some(fs) = &surface_ctx.fractional_scale {
            builder = builder.with_fractional_scale(Rc::clone(fs));
        }

        if let Some(vp) = &surface_ctx.viewport {
            builder = builder.with_viewport(Rc::clone(vp));
        }

        let mut surface_state = SurfaceState::new(builder)
            .map_err(|e| PlatformError::Other(format!("Surface init failed: {e}")))?;
        surface_state.set_shared_pointer_serial(Rc::clone(&shared_serial));

        let output_id = output.id();
        let surface_handle = SurfaceHandle::new();
        let surface_name = "slint-window".to_string();

        system.app_state_mut().add_shell_surface(
            &output_id,
            surface_handle,
            &surface_name,
            main_surface_id.clone(),
            surface_state,
        );

        let entry = LayerShellSurfaceEntry {
            surface_id: main_surface_id.clone(),
            surface: Rc::clone(&surface_ctx.surface),
            layer_surface: Rc::clone(&surface_ctx.layer_surface),
            window: Rc::clone(&window),
        };
        self.config_state.borrow_mut().register_surface(entry);
        RenderableWindow::request_redraw(window.as_ref());

        let config_state = Rc::clone(&self.config_state);
        window.set_window_properties_handler(Rc::new(move |properties: WindowProperties<'_>| {
            let title = properties.title();
            if title.is_empty() {
                return;
            }
            let preferred = properties.layout_constraints().preferred;
            let mut state = config_state.borrow_mut();
            state.register_title(&main_surface_id, title.as_ref());
            state.update_preferred_size(&title, preferred);
        }));

        let window_adapter = window as Rc<dyn WindowAdapter>;
        self.pending_windows
            .borrow_mut()
            .push(Rc::clone(&window_adapter));

        Ok(window_adapter)
    }

    fn run_event_loop(&self) -> Result<(), PlatformError> {
        logger::info!("Starting LayerShell event loop");
        self.ensure_system()?;

        let mut system_ref = self.system.borrow_mut();
        let system = system_ref
            .as_mut()
            .ok_or_else(|| PlatformError::Other("Wayland system unavailable".to_string()))?;

        self.pending_windows.borrow_mut().clear();
        system
            .run()
            .map_err(|e| PlatformError::Other(format!("Event loop failed: {e}")))
    }
}

pub struct LayerShellWindow {
    title: String,
    config_state: Rc<RefCell<LayerShellConfigState>>,
    namespaces: Rc<RefCell<OrderMap<String, String>>>,
    output_policies: Rc<RefCell<OrderMap<String, OutputPolicy>>>,
}

impl LayerShellWindow {
    pub fn namespace(self, namespace: impl Into<String>) -> Self {
        let namespace = namespace.into();
        self.namespaces
            .borrow_mut()
            .insert(self.title.clone(), namespace);
        self
    }

    pub fn output_policy(self, policy: OutputPolicy) -> Self {
        self.output_policies
            .borrow_mut()
            .insert(self.title.clone(), policy);
        self
    }

    pub fn anchor(self, anchor: AnchorEdges) -> Self {
        self.config_state
            .borrow_mut()
            .set_for_title(&self.title, |config| config.anchor = anchor);
        self
    }

    pub fn exclusive_zone(self, zone: i32) -> Self {
        self.config_state
            .borrow_mut()
            .set_for_title(&self.title, |config| config.exclusive_zone = Some(zone));
        self
    }

    pub fn layer(self, layer: Layer) -> Self {
        self.config_state
            .borrow_mut()
            .set_for_title(&self.title, |config| config.layer = layer);
        self
    }

    pub fn margins(self, margins: Margins) -> Self {
        self.config_state
            .borrow_mut()
            .set_for_title(&self.title, |config| config.margin = margins);
        self
    }

    pub fn keyboard_interactivity(self, mode: KeyboardInteractivity) -> Self {
        self.config_state
            .borrow_mut()
            .set_for_title(&self.title, |config| config.keyboard_interactivity = mode);
        self
    }

    pub fn size(self, width: u32, height: u32) -> Self {
        self.config_state
            .borrow_mut()
            .set_for_title(&self.title, |config| {
                config.width = Some(width);
                config.height = Some(height);
            });
        self
    }

    pub fn scale_factor(self, scale_factor: f32) -> Self {
        self.config_state
            .borrow_mut()
            .set_for_title(&self.title, |config| config.scale_factor = scale_factor);
        self
    }
}
