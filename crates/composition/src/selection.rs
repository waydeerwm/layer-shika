use layer_shika_domain::errors::DomainError;
use layer_shika_domain::value_objects::surface_instance_id::SurfaceInstanceId;

use crate::selector::{Selector, SurfaceInfo};
use crate::slint_interpreter::{ComponentInstance, Value};
use crate::{Error, LayerSurfaceHandle, Shell, logger};

/// Result of a property operation on a single surface
#[derive(Debug)]
pub struct PropertyError {
    pub surface_name: String,
    pub instance_id: SurfaceInstanceId,
    pub error: String,
}

/// Result of an operation on multiple selected surfaces
#[derive(Debug)]
pub struct SelectionResult<T> {
    pub success_count: usize,
    pub values: Vec<T>,
    pub failures: Vec<PropertyError>,
}

impl<T> SelectionResult<T> {
    fn new() -> Self {
        Self {
            success_count: 0,
            values: Vec::new(),
            failures: Vec::new(),
        }
    }

    fn add_success(&mut self, value: T) {
        self.success_count += 1;
        self.values.push(value);
    }

    fn add_failure(&mut self, surface_name: String, instance_id: SurfaceInstanceId, error: String) {
        self.failures.push(PropertyError {
            surface_name,
            instance_id,
            error,
        });
    }

    pub fn is_ok(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn is_partial_failure(&self) -> bool {
        !self.failures.is_empty() && self.success_count > 0
    }

    pub fn is_total_failure(&self) -> bool {
        !self.failures.is_empty() && self.success_count == 0
    }

    pub fn into_result(self) -> Result<Vec<T>, Error> {
        if self.failures.is_empty() {
            Ok(self.values)
        } else {
            let error_messages: Vec<String> = self
                .failures
                .iter()
                .map(|e| format!("{}[{:?}]: {}", e.surface_name, e.instance_id, e.error))
                .collect();
            Err(Error::Domain(DomainError::Configuration {
                message: format!(
                    "Operation failed on {} surface(s): {}",
                    self.failures.len(),
                    error_messages.join(", ")
                ),
            }))
        }
    }
}

/// A selection of surfaces matching a selector
///
/// Provides methods to interact with all matching surfaces at once.
/// Created via `Shell::select()`.
pub struct Selection<'a> {
    shell: &'a Shell,
    selector: Selector,
}

impl<'a> Selection<'a> {
    pub(crate) fn new(shell: &'a Shell, selector: Selector) -> Self {
        Self { shell, selector }
    }

    /// Registers a callback handler for all matching surfaces
    ///
    /// ```ignore
    /// shell.select(Surface::named("bar"))
    ///     .on_callback("clicked", |ctx| {
    ///         println!("Clicked: {}", ctx.surface_name());
    ///     });
    /// ```
    pub fn on_callback<F, R>(&self, callback_name: &str, handler: F) -> &Self
    where
        F: Fn(crate::CallbackContext) -> R + Clone + 'static,
        R: crate::IntoValue,
    {
        self.shell
            .on_internal(&self.selector, callback_name, handler);
        self
    }

    /// Registers a callback handler that receives Slint arguments
    ///
    /// ```ignore
    /// // Slint: callback item-clicked(string);
    /// shell.select(Surface::named("menu"))
    ///     .on_callback_with_args("item-clicked", |args, ctx| {
    ///         if let Some(Value::String(item)) = args.get(0) {
    ///             println!("{} clicked {}", ctx.surface_name(), item);
    ///         }
    ///     });
    /// ```
    pub fn on_callback_with_args<F, R>(&self, callback_name: &str, handler: F) -> &Self
    where
        F: Fn(&[Value], crate::CallbackContext) -> R + Clone + 'static,
        R: crate::IntoValue,
    {
        self.shell
            .on_with_args_internal(&self.selector, callback_name, handler);
        self
    }

    /// Executes a function with each matching component instance
    pub fn with_component<F>(&self, mut f: F)
    where
        F: FnMut(&ComponentInstance),
    {
        self.shell.with_selected(&self.selector, |_, component| {
            f(component);
        });
    }

    /// Sets a property value on all matching surfaces
    ///
    /// Returns a `SelectionResult` that contains information about both successes and failures.
    /// Use `.into_result()` to convert to a standard `Result` if you want fail-fast behavior,
    /// or inspect `.failures` to handle partial failures gracefully.
    pub fn set_property(&self, name: &str, value: &Value) -> SelectionResult<()> {
        let mut result = SelectionResult::new();
        self.shell
            .with_selected_info(&self.selector, |info, component| {
                match component.set_property(name, value.clone()) {
                    Ok(()) => result.add_success(()),
                    Err(e) => {
                        let error_msg = format!("Failed to set property '{}': {}", name, e);
                        logger::error!(
                            "{} on surface {}[{:?}]",
                            error_msg,
                            info.name,
                            info.instance_id
                        );
                        result.add_failure(info.name.clone(), info.instance_id, error_msg);
                    }
                }
            });
        result
    }

    /// Gets property values from all matching surfaces
    ///
    /// Returns a `SelectionResult` containing all successfully retrieved values and any failures.
    /// Use `.into_result()` to convert to a standard `Result` if you want fail-fast behavior,
    /// or inspect `.values` and `.failures` to handle partial failures gracefully.
    pub fn get_property(&self, name: &str) -> SelectionResult<Value> {
        let mut result = SelectionResult::new();
        self.shell
            .with_selected_info(&self.selector, |info, component| {
                match component.get_property(name) {
                    Ok(value) => result.add_success(value),
                    Err(e) => {
                        let error_msg = format!("Failed to get property '{}': {}", name, e);
                        logger::error!(
                            "{} on surface {}[{:?}]",
                            error_msg,
                            info.name,
                            info.instance_id
                        );
                        result.add_failure(info.name.clone(), info.instance_id, error_msg);
                    }
                }
            });
        result
    }

    /// Executes a configuration function with component and surface handle for matching surfaces
    pub fn configure<F>(&self, mut f: F)
    where
        F: FnMut(&ComponentInstance, LayerSurfaceHandle<'_>),
    {
        self.shell
            .configure_selected(&self.selector, |component, handle| {
                f(component, handle);
            });
    }

    /// Returns the number of surfaces matching the selector
    pub fn count(&self) -> usize {
        self.shell.count_selected(&self.selector)
    }

    /// Checks if no surfaces match the selector
    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }

    /// Returns information about all matching surfaces
    pub fn info(&self) -> Vec<SurfaceInfo> {
        self.shell.get_selected_info(&self.selector)
    }
}
