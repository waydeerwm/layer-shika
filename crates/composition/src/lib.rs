#![allow(clippy::pub_use)]

mod event_loop;
mod layer_surface;
mod lock_selection;
mod popup;
mod popup_builder;
mod selection;
mod selector;
mod session_lock;
mod shell;
mod shell_config;
mod shell_runtime;
mod surface_registry;
mod system;
pub mod value_conversion;

use layer_shika_adapters::errors::LayerShikaError;
use layer_shika_domain::errors::DomainError;
use std::result::Result as StdResult;

pub use event_loop::{EventLoopHandle, ShellEventLoop};
pub use layer_shika_adapters::PopupWindow;
pub use layer_shika_adapters::platform::{slint, slint_interpreter};
pub use layer_shika_domain::entities::output_registry::OutputRegistry;
pub use layer_shika_domain::prelude::AnchorStrategy;
pub use layer_shika_domain::value_objects::anchor::AnchorEdges;
pub use layer_shika_domain::value_objects::handle::{Handle, PopupHandle, SurfaceHandle};
pub use layer_shika_domain::value_objects::keyboard_interactivity::KeyboardInteractivity;
pub use layer_shika_domain::value_objects::layer::Layer;
pub use layer_shika_domain::value_objects::output_handle::OutputHandle;
pub use layer_shika_domain::value_objects::output_info::{OutputGeometry, OutputInfo};
pub use layer_shika_domain::value_objects::output_policy::OutputPolicy;
pub use layer_shika_domain::value_objects::surface_instance_id::SurfaceInstanceId;
pub use layer_shika_domain::value_objects::{
    output_target::OutputTarget,
    popup_behavior::{ConstraintAdjustment, OutputMigrationPolicy, PopupBehavior},
    popup_config::PopupConfig,
    popup_position::{Alignment, AnchorPoint, Offset, PopupPosition},
    popup_size::PopupSize,
};
pub use layer_surface::{LayerSurfaceHandle, ShellSurfaceConfigHandler};
pub use lock_selection::LockSelection;
pub use popup::PopupShell;
pub use popup_builder::{Bound, PopupBuilder, Unbound};
pub use selection::{PropertyError, Selection, SelectionResult};
pub use selector::{Output, Selector, Surface, SurfaceInfo};
pub use session_lock::{SessionLock, SessionLockBuilder};
pub use shell_runtime::{DEFAULT_SURFACE_NAME, ShellRuntime};
pub use system::{
    CallbackContext, EventDispatchContext, RuntimeSurfaceConfigBuilder, ShellControl,
    SurfaceControlHandle, SurfaceTarget,
};
pub use value_conversion::IntoValue;

pub use shell::{
    DEFAULT_COMPONENT_NAME, Shell, ShellBuilder, ShellEventContext, SurfaceConfigBuilder,
};

pub use surface_registry::{SurfaceDefinition, SurfaceEntry, SurfaceMetadata, SurfaceRegistry};

pub use shell_config::{CompiledUiSource, ShellConfig, SurfaceComponentConfig};

pub(crate) mod logger {
    #[cfg(all(feature = "use-log", feature = "use-tracing"))]
    compile_error!("Cannot use both logging backend at one time");

    #[cfg(feature = "use-log")]
    pub use log::{debug, error, info, warn};

    #[cfg(feature = "use-tracing")]
    pub use tracing::{debug, error, info, warn};
}

pub mod calloop {
    pub use layer_shika_adapters::platform::calloop::*;
}

/// Result type alias using layer-shika's Error
pub type Result<T> = StdResult<T, Error>;

/// Error types for layer-shika operations
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Adapter error: {0}")]
    Adapter(#[from] LayerShikaError),

    #[error("Domain error: {0}")]
    Domain(#[from] DomainError),

    #[error("App has been dropped")]
    SystemDropped,

    #[error("Invalid lock state '{current}' for operation '{operation}'")]
    InvalidState { current: String, operation: String },

    #[error("Protocol '{protocol}' not available on this compositor")]
    ProtocolNotAvailable { protocol: String },
}

pub mod prelude {
    pub use crate::{
        AnchorEdges, AnchorStrategy, CompiledUiSource, DEFAULT_COMPONENT_NAME,
        DEFAULT_SURFACE_NAME, EventDispatchContext, EventLoopHandle, Handle, IntoValue,
        KeyboardInteractivity, Layer, LayerSurfaceHandle, LockSelection, Output, OutputGeometry,
        OutputHandle, OutputInfo, OutputPolicy, OutputRegistry, PopupBuilder, PopupConfig,
        PopupHandle, PopupPosition, PopupShell, PopupSize, PopupWindow, PropertyError, Result,
        Selection, SelectionResult, Selector, SessionLock, SessionLockBuilder, Shell, ShellBuilder,
        ShellConfig, ShellControl, ShellEventContext, ShellEventLoop, ShellRuntime,
        ShellSurfaceConfigHandler, Surface, SurfaceComponentConfig, SurfaceConfigBuilder,
        SurfaceControlHandle, SurfaceDefinition, SurfaceEntry, SurfaceHandle, SurfaceInfo,
        SurfaceMetadata, SurfaceRegistry,
    };

    pub use crate::calloop::{Generic, Interest, Mode, PostAction, RegistrationToken, Timer};

    pub use crate::{slint, slint_interpreter};

    pub use layer_shika_domain::prelude::{
        LogicalPosition, LogicalRect, LogicalSize, Margins, PhysicalSize, ScaleFactor,
        SurfaceConfig, SurfaceDimension, UiSource,
    };

    pub use layer_shika_adapters::platform::wayland::Anchor;
}
