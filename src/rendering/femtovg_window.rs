use log::info;
use slint::{
    platform::{femtovg_renderer::FemtoVGRenderer, Renderer, WindowAdapter, WindowEvent},
    PhysicalSize, Window, WindowSize,
};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

pub struct FemtoVGWindow {
    window: RefCell<Window>,
    renderer: RefCell<FemtoVGRenderer>,
    is_dirty: Cell<bool>,
    size: Cell<PhysicalSize>,
    scale_factor: Cell<f32>,
}

impl FemtoVGWindow {
    pub fn new(renderer: FemtoVGRenderer) -> Rc<Self> {
        Rc::new_cyclic(|weak_self| {
            let window = Window::new(weak_self.clone() as Weak<dyn WindowAdapter>);
            Self {
                window: RefCell::new(window),
                renderer: RefCell::new(renderer),
                is_dirty: Default::default(),
                size: Cell::new(PhysicalSize::default()),
                scale_factor: Cell::new(1.),
            }
        })
    }

    pub fn render_frame_if_dirty(&self) {
        if self.is_dirty.get() {
            match self.renderer.borrow_mut().render() {
                Ok(_) => {} //log::debug!("Frame rendered successfully"),
                Err(e) => log::error!("Error rendering frame: {}", e),
            }
            self.is_dirty.set(false);
        }
    }

    pub fn set_scale_factor(&self, scale_factor: f32) {
        info!("Setting scale factor to {}", scale_factor);
        self.scale_factor.set(scale_factor);
        self.window()
            .dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor });
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor.get()
    }
}

impl WindowAdapter for FemtoVGWindow {
    fn window(&self) -> &Window {
        unsafe { self.window.as_ptr().as_ref().unwrap() }
    }

    fn renderer(&self) -> &dyn Renderer {
        unsafe { &*self.renderer.as_ptr() }
    }

    fn size(&self) -> PhysicalSize {
        self.size.get()
    }

    fn set_size(&self, size: WindowSize) {
        self.size.set(size.to_physical(self.scale_factor()));
        self.window.borrow().dispatch_event(WindowEvent::Resized {
            size: size.to_logical(self.scale_factor()),
        });
    }

    fn request_redraw(&self) {
        self.is_dirty.set(true);
    }
}

impl core::ops::Deref for FemtoVGWindow {
    type Target = Window;
    fn deref(&self) -> &Self::Target {
        unsafe { self.window.as_ptr().as_ref().unwrap() }
    }
}
