use std::rc::Rc;

use layer_shika_domain::value_objects::output_handle::OutputHandle;
use layer_shika_domain::value_objects::output_info::OutputInfo;
use slint_interpreter::{ComponentInstance, Value};

use crate::errors::{LayerShikaError, Result};

pub(crate) trait FilterContext {
    fn matches_filter(&self, filter: &dyn Fn(&Self) -> bool) -> bool {
        filter(self)
    }
}

type FilterFn<Ctx> = Rc<dyn Fn(&Ctx) -> bool>;

pub(crate) struct CallbackEntry<Ctx: FilterContext, Handler> {
    name: String,
    handler: Handler,
    filter: Option<FilterFn<Ctx>>,
}

impl<Ctx: FilterContext, Handler: Clone> CallbackEntry<Ctx, Handler> {
    fn new(name: impl Into<String>, handler: Handler) -> Self {
        Self {
            name: name.into(),
            handler,
            filter: None,
        }
    }

    fn with_filter<F>(name: impl Into<String>, handler: Handler, filter: F) -> Self
    where
        F: Fn(&Ctx) -> bool + 'static,
    {
        Self {
            name: name.into(),
            handler,
            filter: Some(Rc::new(filter)),
        }
    }

    pub fn should_apply(&self, context: &Ctx) -> bool {
        self.filter
            .as_ref()
            .is_none_or(|f| context.matches_filter(f.as_ref()))
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn handler(&self) -> &Handler {
        &self.handler
    }
}

impl<Ctx: FilterContext, Handler: Clone> Clone for CallbackEntry<Ctx, Handler> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            handler: self.handler.clone(),
            filter: self.filter.clone(),
        }
    }
}

pub type CallbackHandler = Rc<dyn Fn(&[Value]) -> Value>;

pub struct LockCallbackContext {
    pub component_name: String,
    pub output_handle: OutputHandle,
    pub output_info: Option<OutputInfo>,
    pub primary_handle: Option<OutputHandle>,
    pub active_handle: Option<OutputHandle>,
}

impl LockCallbackContext {
    pub fn new(
        component_name: String,
        output_handle: OutputHandle,
        output_info: Option<OutputInfo>,
        primary_handle: Option<OutputHandle>,
        active_handle: Option<OutputHandle>,
    ) -> Self {
        Self {
            component_name,
            output_handle,
            output_info,
            primary_handle,
            active_handle,
        }
    }
}

impl FilterContext for LockCallbackContext {}

pub type LockCallbackEntry = CallbackEntry<LockCallbackContext, CallbackHandler>;

pub type LockCallback = LockCallbackEntry;

pub type OutputFilter = Rc<
    dyn Fn(
        &str,
        OutputHandle,
        Option<&OutputInfo>,
        Option<OutputHandle>,
        Option<OutputHandle>,
    ) -> bool,
>;

pub fn create_lock_callback(name: impl Into<String>, handler: CallbackHandler) -> LockCallback {
    LockCallbackEntry::new(name, handler)
}

pub fn create_lock_callback_with_output_filter<F>(
    name: impl Into<String>,
    handler: CallbackHandler,
    output_filter: F,
) -> LockCallback
where
    F: Fn(
            &str,
            OutputHandle,
            Option<&OutputInfo>,
            Option<OutputHandle>,
            Option<OutputHandle>,
        ) -> bool
        + 'static,
{
    LockCallbackEntry::with_filter(name, handler, move |ctx: &LockCallbackContext| {
        output_filter(
            &ctx.component_name,
            ctx.output_handle,
            ctx.output_info.as_ref(),
            ctx.primary_handle,
            ctx.active_handle,
        )
    })
}

pub trait LockCallbackExt {
    fn apply_to_component(&self, component: &ComponentInstance) -> Result<()>;
    fn apply_with_context(
        &self,
        component: &ComponentInstance,
        context: &LockCallbackContext,
    ) -> Result<()>;
}

impl LockCallbackExt for LockCallbackEntry {
    fn apply_to_component(&self, component: &ComponentInstance) -> Result<()> {
        let handler = Rc::clone(self.handler());
        component
            .set_callback(self.name(), move |args| handler(args))
            .map_err(|e| LayerShikaError::InvalidInput {
                message: format!("Failed to register callback '{}': {e}", self.name()),
            })
    }

    fn apply_with_context(
        &self,
        component: &ComponentInstance,
        context: &LockCallbackContext,
    ) -> Result<()> {
        if !self.should_apply(context) {
            return Ok(());
        }

        self.apply_to_component(component)
    }
}

pub struct LockPropertyOperation {
    name: String,
    value: Value,
    filter: Option<FilterFn<LockCallbackContext>>,
}

impl LockPropertyOperation {
    pub fn new(name: impl Into<String>, value: Value) -> Self {
        Self {
            name: name.into(),
            value,
            filter: None,
        }
    }

    pub fn with_filter<F>(name: impl Into<String>, value: Value, filter: F) -> Self
    where
        F: Fn(&LockCallbackContext) -> bool + 'static,
    {
        Self {
            name: name.into(),
            value,
            filter: Some(Rc::new(filter)),
        }
    }

    pub fn should_apply(&self, context: &LockCallbackContext) -> bool {
        self.filter
            .as_ref()
            .is_none_or(|f| context.matches_filter(f.as_ref()))
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> &Value {
        &self.value
    }
}

impl Clone for LockPropertyOperation {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            value: self.value.clone(),
            filter: self.filter.clone(),
        }
    }
}

pub fn create_lock_property_operation_with_output_filter<F>(
    name: impl Into<String>,
    value: Value,
    output_filter: F,
) -> LockPropertyOperation
where
    F: Fn(
            &str,
            OutputHandle,
            Option<&OutputInfo>,
            Option<OutputHandle>,
            Option<OutputHandle>,
        ) -> bool
        + 'static,
{
    LockPropertyOperation::with_filter(name, value, move |ctx: &LockCallbackContext| {
        output_filter(
            &ctx.component_name,
            ctx.output_handle,
            ctx.output_info.as_ref(),
            ctx.primary_handle,
            ctx.active_handle,
        )
    })
}

pub trait LockPropertyOperationExt {
    fn apply_to_component(&self, component: &ComponentInstance) -> Result<()>;
    fn apply_with_context(
        &self,
        component: &ComponentInstance,
        context: &LockCallbackContext,
    ) -> Result<()>;
}

impl LockPropertyOperationExt for LockPropertyOperation {
    fn apply_to_component(&self, component: &ComponentInstance) -> Result<()> {
        component
            .set_property(self.name(), self.value().clone())
            .map_err(|e| LayerShikaError::InvalidInput {
                message: format!("Failed to set property '{}': {e}", self.name()),
            })
    }

    fn apply_with_context(
        &self,
        component: &ComponentInstance,
        context: &LockCallbackContext,
    ) -> Result<()> {
        if !self.should_apply(context) {
            return Ok(());
        }

        self.apply_to_component(component)
    }
}
