use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::sync::Arc;

use layer_shika_domain::value_objects::surface_instance_id::SurfaceInstanceId;

use crate::{OutputHandle, OutputInfo};

/// Runtime information about a surface instance
#[derive(Debug, Clone)]
pub struct SurfaceInfo {
    /// Surface component name
    pub name: String,
    /// Handle to the output displaying this surface
    pub output: OutputHandle,
    /// Unique identifier for this surface instance
    pub instance_id: SurfaceInstanceId,
}

/// Selector for targeting surfaces in callbacks and runtime configuration
///
/// # Examples
///
/// ```ignore
/// // Single surface by name
/// shell.select(Surface::named("bar"))
///     .on_callback("clicked", |ctx| { /* ... */ });
///
/// // Multiple surfaces
/// shell.select(Surface::any(["bar", "panel"]))
///     .set_property("visible", &Value::Bool(true));
///
/// // All except one
/// shell.select(Surface::all().except(Surface::named("hidden")))
///     .on_callback("update", |ctx| { /* ... */ });
///
/// // Custom filter
/// shell.select(Surface::matching(|info| info.name.starts_with("popup")))
///     .set_property("layer", &Value::String("overlay".into()));
///
/// // Combine with output selector
/// Surface::named("bar").on(Output::primary())
/// ```
#[derive(Clone)]
pub enum Surface {
    /// Select all surfaces
    All,
    /// Select surface by exact name
    Named(String),
    /// Select any surface matching one of the given names
    Any(Vec<String>),
    /// Select surfaces matching a custom predicate
    Filter(Arc<dyn Fn(&SurfaceInfo) -> bool + Send + Sync>),
    /// Invert selection
    Not(Box<Surface>),
    /// Union of multiple selectors
    Or(Vec<Surface>),
}

impl Surface {
    /// Selects all surfaces
    pub fn all() -> Self {
        Self::All
    }

    /// Selects a surface by exact name
    pub fn named(name: impl Into<String>) -> Self {
        Self::Named(name.into())
    }

    /// Selects surfaces matching any of the given names
    pub fn any(names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self::Any(names.into_iter().map(Into::into).collect())
    }

    /// Selects surfaces matching a custom predicate
    ///
    /// ```ignore
    /// Surface::matching(|info| info.name.starts_with("widget"))
    /// ```
    pub fn matching<F>(predicate: F) -> Self
    where
        F: Fn(&SurfaceInfo) -> bool + Send + Sync + 'static,
    {
        Self::Filter(Arc::new(predicate))
    }

    /// Combines this surface selector with an output selector
    ///
    /// ```ignore
    /// Surface::named("bar").on(Output::primary())
    /// Surface::all().on(Output::named("HDMI-1"))
    /// ```
    pub fn on(self, output: Output) -> Selector {
        Selector {
            surface: self,
            output,
        }
    }

    /// Inverts the selection to exclude matching surfaces
    ///
    /// ```ignore
    /// Surface::all().except(Surface::named("hidden"))
    /// ```
    #[must_use]
    pub fn except(self, other: impl Into<Surface>) -> Self {
        Self::Not(Box::new(other.into()))
    }

    /// Combines this selector with another using OR logic
    ///
    /// ```ignore
    /// Surface::named("bar").or(Surface::named("panel"))
    /// ```
    #[must_use]
    pub fn or(self, other: impl Into<Surface>) -> Self {
        match self {
            Self::Or(mut selectors) => {
                selectors.push(other.into());
                Self::Or(selectors)
            }
            _ => Self::Or(vec![self, other.into()]),
        }
    }

    pub(crate) fn matches(&self, info: &SurfaceInfo) -> bool {
        match self {
            Self::All => true,
            Self::Named(name) => &info.name == name,
            Self::Any(names) => names.iter().any(|name| name == &info.name),
            Self::Filter(predicate) => predicate(info),
            Self::Not(selector) => !selector.matches(info),
            Self::Or(selectors) => selectors.iter().any(|s| s.matches(info)),
        }
    }
}

impl Debug for Surface {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::All => write!(f, "Surface::All"),
            Self::Named(name) => write!(f, "Surface::Named({:?})", name),
            Self::Any(names) => write!(f, "Surface::Any({:?})", names),
            Self::Filter(_) => write!(f, "Surface::Filter(<fn>)"),
            Self::Not(selector) => write!(f, "Surface::Not({:?})", selector),
            Self::Or(selectors) => write!(f, "Surface::Or({:?})", selectors),
        }
    }
}

/// Selector for targeting outputs (monitors)
///
/// # Examples
///
/// ```ignore
/// // Primary monitor only
/// shell.select(Surface::named("bar").on(Output::primary()))
///     .on_callback("clicked", |ctx| { /* ... */ });
///
/// // Specific output by name
/// shell.select(Output::named("HDMI-1"))
///     .set_property("scale", &Value::Number(1.5));
///
/// // Custom filter
/// shell.select(Output::matching(|info| info.scale().unwrap_or(1) > 1))
///     .set_property("enable-hidpi", &Value::Bool(true));
/// ```
#[derive(Clone)]
pub enum Output {
    /// Select all outputs
    All,
    /// Select the primary output
    Primary,
    /// Select the currently active output
    Active,
    /// Select output by handle
    Handle(OutputHandle),
    /// Select output by name
    Named(String),
    /// Select outputs matching a custom predicate
    Filter(Arc<dyn Fn(&OutputInfo) -> bool + Send + Sync>),
    /// Invert selection
    Not(Box<Output>),
    /// Union of multiple selectors
    Or(Vec<Output>),
}

impl Output {
    /// Selects all outputs
    pub fn all() -> Self {
        Self::All
    }

    /// Selects the primary output
    pub fn primary() -> Self {
        Self::Primary
    }

    /// Selects the currently active output
    pub fn active() -> Self {
        Self::Active
    }

    /// Selects an output by handle
    pub fn handle(handle: OutputHandle) -> Self {
        Self::Handle(handle)
    }

    /// Selects an output by name
    pub fn named(name: impl Into<String>) -> Self {
        Self::Named(name.into())
    }

    /// Selects outputs matching a custom predicate
    ///
    /// ```ignore
    /// Output::matching(|info| info.is_primary())
    /// ```
    pub fn matching<F>(predicate: F) -> Self
    where
        F: Fn(&OutputInfo) -> bool + Send + Sync + 'static,
    {
        Self::Filter(Arc::new(predicate))
    }

    /// Inverts the selection to exclude matching outputs
    ///
    /// ```ignore
    /// Output::all().except(Output::primary())
    /// ```
    #[must_use]
    pub fn except(self, other: impl Into<Output>) -> Self {
        Self::Not(Box::new(other.into()))
    }

    /// Combines this selector with another using OR logic
    ///
    /// ```ignore
    /// Output::named("HDMI-1").or(Output::named("HDMI-2"))
    /// ```
    #[must_use]
    pub fn or(self, other: impl Into<Output>) -> Self {
        match self {
            Self::Or(mut selectors) => {
                selectors.push(other.into());
                Self::Or(selectors)
            }
            _ => Self::Or(vec![self, other.into()]),
        }
    }

    pub(crate) fn matches(
        &self,
        handle: OutputHandle,
        info: Option<&OutputInfo>,
        primary: Option<OutputHandle>,
        active: Option<OutputHandle>,
    ) -> bool {
        match self {
            Self::All => true,
            Self::Primary => primary == Some(handle),
            Self::Active => active == Some(handle),
            Self::Handle(h) => *h == handle,
            Self::Named(name) => info.is_some_and(|i| i.name() == Some(name.as_str())),
            Self::Filter(predicate) => info.is_some_and(|i| predicate(i)),
            Self::Not(selector) => !selector.matches(handle, info, primary, active),
            Self::Or(selectors) => selectors
                .iter()
                .any(|s| s.matches(handle, info, primary, active)),
        }
    }
}

impl Debug for Output {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::All => write!(f, "Output::All"),
            Self::Primary => write!(f, "Output::Primary"),
            Self::Active => write!(f, "Output::Active"),
            Self::Handle(h) => write!(f, "Output::Handle({:?})", h),
            Self::Named(name) => write!(f, "Output::Named({:?})", name),
            Self::Filter(_) => write!(f, "Output::Filter(<fn>)"),
            Self::Not(selector) => write!(f, "Output::Not({:?})", selector),
            Self::Or(selectors) => write!(f, "Output::Or({:?})", selectors),
        }
    }
}

/// Combined surface and output selector for precise targeting
///
/// Created by combining `Surface` and `Output` selectors:
///
/// ```ignore
/// Surface::named("bar").on(Output::primary())
/// ```
///
/// Or implicitly from either selector:
///
/// ```ignore
/// shell.select(Surface::named("bar"))  // All outputs
/// shell.select(Output::primary())      // All surfaces
/// ```
#[derive(Clone, Debug)]
pub struct Selector {
    pub surface: Surface,
    pub output: Output,
}

impl Selector {
    /// Creates a selector matching all surfaces on all outputs
    pub fn all() -> Self {
        Self {
            surface: Surface::All,
            output: Output::All,
        }
    }

    pub(crate) fn matches(
        &self,
        surface_info: &SurfaceInfo,
        output_info: Option<&OutputInfo>,
        primary: Option<OutputHandle>,
        active: Option<OutputHandle>,
    ) -> bool {
        self.surface.matches(surface_info)
            && self
                .output
                .matches(surface_info.output, output_info, primary, active)
    }
}

impl From<Surface> for Selector {
    fn from(surface: Surface) -> Self {
        Self {
            surface,
            output: Output::All,
        }
    }
}

impl From<Output> for Selector {
    fn from(output: Output) -> Self {
        Self {
            surface: Surface::All,
            output,
        }
    }
}
