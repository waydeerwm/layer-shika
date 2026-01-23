use crate::IntoValue;
use crate::calloop::channel;
use crate::logger;
use crate::slint_interpreter::Value;
use crate::system::{SessionLockCommand, ShellCommand};
use crate::{Error, Result};
use layer_shika_adapters::WaylandSystemOps;
use layer_shika_domain::dimensions::ScaleFactor;
use layer_shika_domain::errors::DomainError;
use layer_shika_domain::value_objects::lock_config::LockConfig;
use layer_shika_domain::value_objects::lock_state::LockState;
use layer_shika_domain::value_objects::margins::Margins;
use layer_shika_domain::value_objects::output_policy::OutputPolicy;
use std::cell::RefCell;
use std::rc::Rc;
use std::rc::Weak;

pub struct SessionLock {
    system: Weak<RefCell<dyn WaylandSystemOps>>,
    component_name: String,
    config: LockConfig,
    command_sender: channel::Sender<ShellCommand>,
}

impl SessionLock {
    pub(crate) fn new(
        system: Weak<RefCell<dyn WaylandSystemOps>>,
        component_name: String,
        config: LockConfig,
        command_sender: channel::Sender<ShellCommand>,
    ) -> Self {
        Self {
            system,
            component_name,
            config,
            command_sender,
        }
    }

    pub fn activate(&self) -> Result<()> {
        logger::info!("Session lock activation called - queuing SessionLockCommand::Activate");

        self.command_sender
            .send(ShellCommand::SessionLock(SessionLockCommand::Activate {
                component_name: self.component_name.clone(),
                config: self.config.clone(),
            }))
            .map_err(|e| {
                Error::Domain(DomainError::InvalidInput {
                    message: format!("Failed to send session lock command: {e:?}"),
                })
            })?;

        logger::info!("SessionLockCommand::Activate queued successfully");
        Ok(())
    }

    pub fn deactivate(&self) -> Result<()> {
        logger::info!("Session lock deactivation called - queuing SessionLockCommand::Deactivate");

        self.command_sender
            .send(ShellCommand::SessionLock(SessionLockCommand::Deactivate))
            .map_err(|e| {
                Error::Domain(DomainError::InvalidInput {
                    message: format!("Failed to send session lock command: {e:?}"),
                })
            })?;

        logger::info!("SessionLockCommand::Deactivate queued successfully");
        Ok(())
    }

    /// Registers a callback handler on the lock screen component.
    ///
    /// The callback must exist in the Slint component, for example:
    /// `callback unlock_requested(string)`.
    pub fn on_callback<F, R>(&self, callback_name: &str, handler: F) -> Result<()>
    where
        F: Fn() -> R + Clone + 'static,
        R: IntoValue,
    {
        self.on_callback_with_args(callback_name, move |_args| handler().into_value())
    }

    /// Registers a callback handler that receives Slint arguments.
    pub fn on_callback_with_args<F, R>(&self, callback_name: &str, handler: F) -> Result<()>
    where
        F: Fn(&[Value]) -> R + Clone + 'static,
        R: IntoValue,
    {
        let system = self.system.upgrade().ok_or(Error::SystemDropped)?;
        let handler = Rc::new(move |args: &[Value]| handler(args).into_value());
        system
            .borrow_mut()
            .register_session_lock_callback(callback_name, handler);
        Ok(())
    }

    #[must_use]
    pub fn state(&self) -> LockState {
        let Some(system) = self.system.upgrade() else {
            return LockState::Inactive;
        };

        let Ok(borrowed) = system.try_borrow() else {
            return LockState::Inactive;
        };

        borrowed.session_lock_state().unwrap_or(LockState::Inactive)
    }

    #[must_use]
    pub fn is_locked(&self) -> bool {
        self.state() == LockState::Locked
    }

    #[must_use]
    pub fn component_name(&self) -> &str {
        &self.component_name
    }
}

pub struct SessionLockBuilder {
    component_name: String,
    config: LockConfig,
}

impl SessionLockBuilder {
    #[must_use]
    pub fn new(component_name: impl Into<String>) -> Self {
        Self {
            component_name: component_name.into(),
            config: LockConfig::default(),
        }
    }

    #[must_use]
    pub fn scale_factor(mut self, factor: impl TryInto<ScaleFactor, Error = DomainError>) -> Self {
        self.config.scale_factor = factor.try_into().unwrap_or_default();
        self
    }

    #[must_use]
    pub fn margin(mut self, margin: impl Into<Margins>) -> Self {
        self.config.margin = margin.into();
        self
    }

    #[must_use]
    pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
        self.config.namespace = namespace.into();
        self
    }

    #[must_use]
    pub fn output_policy(mut self, policy: OutputPolicy) -> Self {
        self.config.output_policy = policy;
        self
    }

    pub(crate) fn build(
        self,
        system: Weak<RefCell<dyn WaylandSystemOps>>,
        command_sender: channel::Sender<ShellCommand>,
    ) -> SessionLock {
        SessionLock::new(system, self.component_name, self.config, command_sender)
    }

    #[must_use]
    pub fn component_name(&self) -> &str {
        &self.component_name
    }
}
