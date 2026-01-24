use std::path::PathBuf;
use std::rc::Rc;

use layer_shika_adapters::platform::slint_interpreter::CompilationResult;
use layer_shika_domain::prelude::{SurfaceConfig, UiSource};

/// Source for Slint UI definition
///
/// Specifies where to load the UI from: a `.slint` file, inline source code, or pre-compiled result.
pub enum CompiledUiSource {
    /// Load UI from a `.slint` file path
    File(PathBuf),
    /// Parse UI from source code string
    Source(String),
    /// Use pre-compiled Slint result
    Compiled(Rc<CompilationResult>),
}

impl CompiledUiSource {
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self::File(path.into())
    }

    pub fn source(code: impl Into<String>) -> Self {
        Self::Source(code.into())
    }

    pub fn compiled(result: Rc<CompilationResult>) -> Self {
        Self::Compiled(result)
    }
}

impl From<UiSource> for CompiledUiSource {
    fn from(source: UiSource) -> Self {
        match source {
            UiSource::File(path) => Self::File(path),
            UiSource::Source(code) => Self::Source(code),
        }
    }
}

impl From<Rc<CompilationResult>> for CompiledUiSource {
    fn from(result: Rc<CompilationResult>) -> Self {
        Self::Compiled(result)
    }
}

impl From<&str> for CompiledUiSource {
    fn from(s: &str) -> Self {
        Self::File(PathBuf::from(s))
    }
}

impl From<String> for CompiledUiSource {
    fn from(s: String) -> Self {
        Self::File(PathBuf::from(s))
    }
}

impl From<PathBuf> for CompiledUiSource {
    fn from(path: PathBuf) -> Self {
        Self::File(path)
    }
}

/// Declarative configuration for creating a shell with multiple surfaces
///
/// Useful for loading configuration from files or programmatic generation.
pub struct ShellConfig {
    pub ui_source: CompiledUiSource,
    pub surfaces: Vec<SurfaceComponentConfig>,
}

/// Associates a Slint component name with its surface configuration
#[derive(Debug, Clone)]
pub struct SurfaceComponentConfig {
    /// Name of the Slint component to instantiate
    pub component: String,
    /// Surface configuration for this component
    pub config: SurfaceConfig,
}

impl ShellConfig {
    /// Creates a new shell configuration from a UI source
    pub fn new(ui_source: impl Into<CompiledUiSource>) -> Self {
        Self {
            ui_source: ui_source.into(),
            surfaces: Vec::new(),
        }
    }

    /// Adds a surface with the given component name
    #[must_use]
    pub fn with_surface(mut self, component: impl Into<String>) -> Self {
        self.surfaces.push(SurfaceComponentConfig {
            component: component.into(),
            config: SurfaceConfig::default(),
        });
        self
    }

    /// Adds a surface with a complete configuration
    #[must_use]
    pub fn with_surface_config(
        mut self,
        component: impl Into<String>,
        config: SurfaceConfig,
    ) -> Self {
        self.surfaces.push(SurfaceComponentConfig {
            component: component.into(),
            config,
        });
        self
    }

    /// Adds a surface and returns a mutable reference to it
    pub fn add_surface(&mut self, component: impl Into<String>) -> &mut SurfaceComponentConfig {
        self.surfaces.push(SurfaceComponentConfig {
            component: component.into(),
            config: SurfaceConfig::default(),
        });
        self.surfaces
            .last_mut()
            .unwrap_or_else(|| unreachable!("just pushed"))
    }

    /// Adds a surface with configuration and returns a mutable reference to it
    pub fn add_surface_config(
        &mut self,
        component: impl Into<String>,
        config: SurfaceConfig,
    ) -> &mut SurfaceComponentConfig {
        self.surfaces.push(SurfaceComponentConfig {
            component: component.into(),
            config,
        });
        self.surfaces
            .last_mut()
            .unwrap_or_else(|| unreachable!("just pushed"))
    }
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            ui_source: CompiledUiSource::Source(String::new()),
            surfaces: Vec::new(),
        }
    }
}

impl SurfaceComponentConfig {
    pub fn new(component: impl Into<String>) -> Self {
        Self {
            component: component.into(),
            config: SurfaceConfig::default(),
        }
    }

    pub fn with_config(component: impl Into<String>, config: SurfaceConfig) -> Self {
        Self {
            component: component.into(),
            config,
        }
    }
}
