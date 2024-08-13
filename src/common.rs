use slint::PhysicalSize;

#[derive(Debug, Clone, Copy)]
pub struct LayerSize {
    size: PhysicalSize,
}

impl LayerSize {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            size: PhysicalSize::new(width, height),
        }
    }

    pub fn physical_size(&self) -> PhysicalSize {
        self.size
    }
}

impl Default for LayerSize {
    fn default() -> Self {
        Self::new(1, 1)
    }
}
