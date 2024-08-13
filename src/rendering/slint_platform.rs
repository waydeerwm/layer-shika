use slint::{
    platform::{Platform, WindowAdapter},
    PlatformError,
};
use std::rc::{Rc, Weak};

use super::femtovg_window::FemtoVGWindow;

pub struct CustomSlintPlatform {
    window: Weak<FemtoVGWindow>,
}

impl CustomSlintPlatform {
    pub fn new(window: &Rc<FemtoVGWindow>) -> Self {
        Self {
            window: Rc::downgrade(window),
        }
    }
}

impl Platform for CustomSlintPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        self.window
            .upgrade()
            .map(|window| -> Rc<dyn WindowAdapter> { window })
            .ok_or_else(|| {
                PlatformError::Other(
                    "Failed to create window adapter: window no longer exists".into(),
                )
            })
    }
}
