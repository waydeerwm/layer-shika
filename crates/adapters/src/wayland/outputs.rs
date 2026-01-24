use std::collections::HashMap;

use layer_shika_domain::value_objects::output_handle::OutputHandle;
pub(crate) use output_manager::{OutputManager, OutputManagerContext};
use wayland_client::backend::ObjectId;
pub(crate) mod output_manager;

pub(crate) struct OutputMapping {
    object_to_handle: HashMap<ObjectId, OutputHandle>,
}

impl OutputMapping {
    pub fn new() -> Self {
        Self {
            object_to_handle: HashMap::new(),
        }
    }

    pub fn insert(&mut self, object_id: ObjectId) -> OutputHandle {
        let handle = OutputHandle::new();
        self.object_to_handle.insert(object_id, handle);
        handle
    }

    pub fn get(&self, object_id: &ObjectId) -> Option<OutputHandle> {
        self.object_to_handle.get(object_id).copied()
    }

    pub fn remove(&mut self, object_id: &ObjectId) -> Option<OutputHandle> {
        self.object_to_handle.remove(object_id)
    }
}

impl Default for OutputMapping {
    fn default() -> Self {
        Self::new()
    }
}
