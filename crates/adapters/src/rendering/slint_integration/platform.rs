use slint::{
    PlatformError,
    platform::{Platform, WindowAdapter},
};
use std::cell::{OnceCell, RefCell};
use std::rc::Rc;

use crate::{logger, rendering::femtovg::main_window::FemtoVGWindow};

type PopupCreator = dyn Fn() -> Result<Rc<dyn WindowAdapter>, PlatformError>;

pub struct CustomSlintPlatform {
    pending_windows: RefCell<Vec<Rc<FemtoVGWindow>>>,
    popup_creator: OnceCell<Rc<PopupCreator>>,
}

impl CustomSlintPlatform {
    #[must_use]
    pub fn new(window: &Rc<FemtoVGWindow>) -> Rc<Self> {
        Rc::new(Self {
            pending_windows: RefCell::new(vec![Rc::clone(window)]),
            popup_creator: OnceCell::new(),
        })
    }

    #[must_use]
    pub fn new_empty() -> Rc<Self> {
        Rc::new(Self {
            pending_windows: RefCell::new(Vec::new()),
            popup_creator: OnceCell::new(),
        })
    }

    pub fn add_window(&self, window: Rc<FemtoVGWindow>) {
        self.pending_windows.borrow_mut().push(window);
    }

    pub fn set_popup_creator<F>(&self, creator: F)
    where
        F: Fn() -> Result<Rc<dyn WindowAdapter>, PlatformError> + 'static,
    {
        if self.popup_creator.set(Rc::new(creator)).is_err() {
            logger::warn!("Popup creator already set, ignoring new creator");
        }
    }
}

impl Platform for CustomSlintPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter + 'static>, PlatformError> {
        let mut windows = self.pending_windows.borrow_mut();
        if !windows.is_empty() {
            let window = windows.remove(0);
            Ok(window as Rc<dyn WindowAdapter>)
        } else if let Some(creator) = self.popup_creator.get() {
            creator()
        } else {
            Err(PlatformError::NoPlatform)
        }
    }
}
