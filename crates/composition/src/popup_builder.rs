use layer_shika_domain::dimensions::LogicalRect;
use layer_shika_domain::value_objects::handle::PopupHandle;
use layer_shika_domain::value_objects::output_target::OutputTarget;
use layer_shika_domain::value_objects::popup_behavior::ConstraintAdjustment;
use layer_shika_domain::value_objects::popup_config::PopupConfig;
use layer_shika_domain::value_objects::popup_position::{
    Alignment, AnchorPoint, Offset, PopupPosition,
};
use layer_shika_domain::value_objects::popup_size::PopupSize;

use crate::Result;
use crate::popup::PopupShell;

/// Type state indicating the builder is not bound to a shell
pub struct Unbound;

/// Type state indicating the builder is bound to a shell
pub struct Bound {
    shell: PopupShell,
}

/// Builder for configuring popups
///
/// The builder uses phantom types to ensure compile-time safety:
/// - [`PopupBuilder<Unbound>`] - Configuration only, cannot show popups
/// - [`PopupBuilder<Bound>`] - Has shell reference, can show popups
///
/// # Example
/// ```rust,ignore
/// shell.on("Main", "open_menu", |control| {
///     let popup_handle = control.popups().builder("MenuPopup")
///         .at_cursor()
///         .grab(true)
///         .close_on("menu_closed")
///         .show()?;
/// });
/// ```
pub struct PopupBuilder<State = Unbound> {
    state: State,
    config: PopupConfig,
}

impl PopupBuilder<Unbound> {
    /// Creates a new popup builder for the specified component
    ///
    /// This builder is unbound and cannot show popups directly.
    /// Use [`PopupShell::builder`] to create a bound builder that can call `.show()`.
    #[must_use]
    pub fn new(component: impl Into<String>) -> Self {
        Self {
            state: Unbound,
            config: PopupConfig::new(component),
        }
    }

    #[must_use]
    pub(crate) fn with_shell(self, shell: PopupShell) -> PopupBuilder<Bound> {
        PopupBuilder {
            state: Bound { shell },
            config: self.config,
        }
    }
}

impl<State> PopupBuilder<State> {
    #[must_use]
    pub fn position(mut self, position: PopupPosition) -> Self {
        self.config.position = position;
        self
    }

    #[must_use]
    pub fn at_cursor(self) -> Self {
        self.position(PopupPosition::Cursor {
            offset: Offset::default(),
        })
    }

    #[must_use]
    pub fn at_position(self, x: f32, y: f32) -> Self {
        self.position(PopupPosition::Absolute { x, y })
    }

    #[must_use]
    pub fn centered(self) -> Self {
        self.position(PopupPosition::Centered {
            offset: Offset::default(),
        })
    }

    #[must_use]
    pub fn relative_to_rect(
        self,
        rect: LogicalRect,
        anchor: AnchorPoint,
        alignment: Alignment,
    ) -> Self {
        self.position(PopupPosition::Element {
            rect,
            anchor,
            alignment,
        })
    }

    #[must_use]
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        match &mut self.config.position {
            PopupPosition::Absolute { x: abs_x, y: abs_y } => {
                *abs_x += x;
                *abs_y += y;
            }
            PopupPosition::Cursor { offset }
            | PopupPosition::Centered { offset }
            | PopupPosition::RelativeToParent { offset, .. } => {
                offset.x += x;
                offset.y += y;
            }
            PopupPosition::Element { .. } => {
                self.config.position = PopupPosition::Cursor {
                    offset: Offset { x, y },
                };
            }
        }
        self
    }

    #[must_use]
    pub fn size(mut self, size: PopupSize) -> Self {
        self.config.size = size;
        self
    }

    #[must_use]
    pub fn fixed_size(self, width: f32, height: f32) -> Self {
        self.size(PopupSize::Fixed { width, height })
    }

    #[must_use]
    pub fn min_size(self, width: f32, height: f32) -> Self {
        self.size(PopupSize::Minimum { width, height })
    }

    #[must_use]
    pub fn max_size(self, width: f32, height: f32) -> Self {
        self.size(PopupSize::Maximum { width, height })
    }

    #[must_use]
    pub fn content_sized(self) -> Self {
        self.size(PopupSize::Content)
    }

    #[must_use]
    pub fn grab(mut self, enable: bool) -> Self {
        self.config.behavior.grab = enable;
        self
    }

    #[must_use]
    pub fn modal(mut self, enable: bool) -> Self {
        self.config.behavior.modal = enable;
        self
    }

    #[must_use]
    pub fn close_on_click_outside(mut self) -> Self {
        self.config.behavior.close_on_click_outside = true;
        self
    }

    #[must_use]
    pub fn close_on_escape(mut self) -> Self {
        self.config.behavior.close_on_escape = true;
        self
    }

    #[must_use]
    pub fn constraint_adjustment(mut self, adjustment: ConstraintAdjustment) -> Self {
        self.config.behavior.constraint_adjustment = adjustment;
        self
    }

    #[must_use]
    pub fn on_output(mut self, target: OutputTarget) -> Self {
        self.config.output = target;
        self
    }

    #[must_use]
    pub fn on_primary(self) -> Self {
        self.on_output(OutputTarget::Primary)
    }

    #[must_use]
    pub fn on_active(self) -> Self {
        self.on_output(OutputTarget::Active)
    }

    #[must_use]
    pub fn parent(mut self, parent: PopupHandle) -> Self {
        self.config.parent = Some(parent);
        self
    }

    #[must_use]
    pub const fn z_index(mut self, index: i32) -> Self {
        self.config.z_index = index;
        self
    }

    #[must_use]
    pub fn close_on(mut self, callback_name: impl Into<String>) -> Self {
        self.config.close_callback = Some(callback_name.into());
        self
    }

    #[must_use]
    pub fn resize_on(mut self, callback_name: impl Into<String>) -> Self {
        self.config.resize_callback = Some(callback_name.into());
        self
    }

    /// Builds the configuration without showing the popup
    ///
    /// Returns a [`PopupConfig`] that can be shown later using [`PopupShell::show`].
    #[must_use]
    pub fn build(self) -> PopupConfig {
        self.config
    }
}

impl PopupBuilder<Bound> {
    /// Shows the popup with the configured settings
    ///
    /// This method is only available on builders created via [`PopupShell::builder`],
    /// ensuring at compile time that the builder has access to a shell.
    pub fn show(self) -> Result<PopupHandle> {
        self.state.shell.show(self.config)
    }
}
