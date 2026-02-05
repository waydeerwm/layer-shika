use std::rc::Rc;

use slint::ComponentHandle;
use slint_interpreter::{CompilationResult, ComponentDefinition, ComponentInstance};

use crate::errors::{LayerShikaError, Result};
use crate::rendering::femtovg::main_window::FemtoVGWindow;

pub struct ComponentState {
    component_instance: ComponentInstance,
    compilation_result: Option<Rc<CompilationResult>>,
}

impl ComponentState {
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(
        component_definition: ComponentDefinition,
        compilation_result: Option<Rc<CompilationResult>>,
        window: &Rc<FemtoVGWindow>,
    ) -> Result<Self> {
        let component_instance = component_definition
            .create()
            .map_err(|e| LayerShikaError::SlintComponentCreation { source: e })?;

        component_instance
            .show()
            .map_err(|e| LayerShikaError::SlintComponentCreation { source: e })?;

        window.request_redraw();

        Ok(Self {
            component_instance,
            compilation_result,
        })
    }

    pub const fn component_instance(&self) -> &ComponentInstance {
        &self.component_instance
    }

    #[must_use]
    pub fn compilation_result(&self) -> Option<Rc<CompilationResult>> {
        self.compilation_result.as_ref().map(Rc::clone)
    }
}
