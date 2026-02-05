use std::error::Error;
use std::result::Result as StdResult;

use layer_shika_domain::errors::DomainError;
use slint::PlatformError;
use slint::platform::SetPlatformError;
use smithay_client_toolkit::reexports::calloop;
use thiserror::Error;
use wayland_client::backend::WaylandError;
use wayland_client::globals::{BindError, GlobalError};
use wayland_client::{ConnectError, DispatchError};

pub type Result<T> = StdResult<T, LayerShikaError>;

#[derive(Error, Debug)]
pub enum RenderingError {
    #[error("failed to swap buffers")]
    SwapBuffers {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },

    #[error("failed to ensure context current")]
    EnsureContextCurrent {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },

    #[error("rendering operation failed: {message}")]
    Operation { message: String },
}

#[derive(Error, Debug)]
pub enum EGLError {
    #[error("failed to create EGL display")]
    DisplayCreation {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },

    #[error("failed to find EGL configurations")]
    ConfigSelection {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },

    #[error("no compatible EGL configurations found")]
    NoCompatibleConfig,

    #[error("failed to create EGL context")]
    ContextCreation {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },

    #[error("failed to create window surface")]
    SurfaceCreation {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },

    #[error("failed to make context current")]
    MakeCurrent {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },

    #[error("failed to swap buffers")]
    SwapBuffers {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
}

#[derive(Error, Debug)]
pub enum EventLoopError {
    #[error("failed to create event loop")]
    Creation {
        #[source]
        source: calloop::Error,
    },

    #[error("failed to insert event source: {message}")]
    InsertSource { message: String },

    #[error("event loop execution failed")]
    Execution {
        #[source]
        source: calloop::Error,
    },
}

#[derive(Error, Debug)]
pub enum LayerShikaError {
    #[error("domain error")]
    Domain {
        #[from]
        source: DomainError,
    },

    #[error("failed to connect to Wayland display")]
    WaylandConnection {
        #[from]
        source: ConnectError,
    },

    #[error("failed to initialize Wayland globals")]
    GlobalInitialization {
        #[source]
        source: GlobalError,
    },

    #[error("Wayland dispatch error")]
    WaylandDispatch {
        #[from]
        source: DispatchError,
    },

    #[error("failed to bind Wayland global")]
    GlobalBind {
        #[from]
        source: BindError,
    },

    #[error("EGL context error")]
    EGLContext {
        #[from]
        source: EGLError,
    },

    #[error("failed to create FemtoVG renderer")]
    FemtoVGRendererCreation {
        #[source]
        source: PlatformError,
    },

    #[error("failed to create Slint component")]
    SlintComponentCreation {
        #[source]
        source: PlatformError,
    },

    #[error("event loop error")]
    EventLoop {
        #[from]
        source: EventLoopError,
    },

    #[error("window configuration error: {message}")]
    WindowConfiguration { message: String },

    #[error("rendering error")]
    Rendering {
        #[from]
        source: RenderingError,
    },

    #[error("invalid input: {message}")]
    InvalidInput { message: String },

    #[error("Wayland protocol error")]
    WaylandProtocol {
        #[source]
        source: WaylandError,
    },

    #[error("failed to set up Slint platform")]
    PlatformSetup {
        #[source]
        source: SetPlatformError,
    },
}
