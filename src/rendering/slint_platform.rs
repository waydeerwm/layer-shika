use anyhow::Result;
use slint::{
    platform::{Platform, WindowAdapter},
    PlatformError,
};
use std::rc::Rc;

use super::femtovg_window::FemtoVGWindow;

pub struct CustomSlintPlatform {
    window: Rc<FemtoVGWindow>,
}

impl CustomSlintPlatform {
    pub fn new(window: Rc<FemtoVGWindow>) -> Self {
        Self { window }
    }
}

impl Platform for CustomSlintPlatform {
    fn create_window_adapter(&self) -> Result<Rc<(dyn WindowAdapter + 'static)>, PlatformError> {
        Result::Ok(Rc::clone(&self.window) as Rc<dyn WindowAdapter>)
    }
}
