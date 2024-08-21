use thiserror::Error;

#[derive(Error, Debug)]
pub enum LayerShikaError {
    #[error("Failed to connect to Wayland: {0}")]
    WaylandConnection(#[from] wayland_client::ConnectError),

    #[error("Failed to initialize Wayland globals: {0}")]
    GlobalInitialization(String),

    #[error("Failed to dispatch Wayland event: {0}")]
    WaylandDispatch(String),

    #[error("Failed to create EGL context: {0}")]
    EGLContextCreation(String),

    #[error("Failed to create FemtoVG renderer: {0}")]
    FemtoVGRendererCreation(String),

    #[error("Failed to create Slint component: {0}")]
    SlintComponentCreation(String),

    #[error("Failed to run event loop: {0}")]
    EventLoop(String),

    #[error("Window configuration error: {0}")]
    WindowConfiguration(String),

    #[error("Rendering error: {0}")]
    Rendering(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Wayland protocol error: {0}")]
    WaylandProtocol(String),

    #[error("Failed to set platform: {0}")]
    PlatformSetup(String),

    #[error("Failed to flush connection: {0}")]
    ConnectionFlush(#[from] wayland_client::backend::WaylandError),
}
