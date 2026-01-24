use std::collections::HashMap;

use crate::value_objects::output_handle::OutputHandle;
use crate::value_objects::output_info::OutputInfo;
use crate::value_objects::output_policy::OutputPolicy;

#[derive(Clone)]
pub struct OutputRegistry {
    outputs: HashMap<OutputHandle, OutputInfo>,
    primary_output: Option<OutputHandle>,
    active_output: Option<OutputHandle>,
}

impl OutputRegistry {
    pub fn new() -> Self {
        Self {
            outputs: HashMap::new(),
            primary_output: None,
            active_output: None,
        }
    }

    pub fn add(&mut self, info: OutputInfo) -> OutputHandle {
        let handle = info.handle();
        let is_first = self.outputs.is_empty();

        self.outputs.insert(handle, info);

        if is_first {
            self.primary_output = Some(handle);
        }

        handle
    }

    pub fn remove(&mut self, handle: OutputHandle) -> Option<OutputInfo> {
        let info = self.outputs.remove(&handle);

        if self.primary_output == Some(handle) {
            self.primary_output = self.outputs.keys().next().copied();
        }

        if self.active_output == Some(handle) {
            self.active_output = None;
        }

        info
    }

    pub fn get(&self, handle: OutputHandle) -> Option<&OutputInfo> {
        self.outputs.get(&handle)
    }

    pub fn get_mut(&mut self, handle: OutputHandle) -> Option<&mut OutputInfo> {
        self.outputs.get_mut(&handle)
    }

    pub fn find_by_name(&self, name: &str) -> Option<(OutputHandle, &OutputInfo)> {
        self.outputs
            .iter()
            .find(|(_, info)| info.name() == Some(name))
            .map(|(handle, info)| (*handle, info))
    }

    pub fn find_by_model(&self, model: &str) -> Option<(OutputHandle, &OutputInfo)> {
        self.outputs
            .iter()
            .find(|(_, info)| info.geometry().and_then(|g| g.model.as_deref()) == Some(model))
            .map(|(handle, info)| (*handle, info))
    }

    pub fn all(&self) -> impl Iterator<Item = (OutputHandle, &OutputInfo)> {
        self.outputs.iter().map(|(handle, info)| (*handle, info))
    }

    pub fn all_info(&self) -> impl Iterator<Item = &OutputInfo> {
        self.outputs.values()
    }

    pub fn primary(&self) -> Option<(OutputHandle, &OutputInfo)> {
        self.primary_output
            .and_then(|handle| self.outputs.get(&handle).map(|info| (handle, info)))
    }

    pub fn primary_handle(&self) -> Option<OutputHandle> {
        self.primary_output
    }

    pub fn active(&self) -> Option<(OutputHandle, &OutputInfo)> {
        self.active_output
            .and_then(|handle| self.outputs.get(&handle).map(|info| (handle, info)))
    }

    pub fn active_handle(&self) -> Option<OutputHandle> {
        self.active_output
    }

    pub fn active_or_primary(&self) -> Option<(OutputHandle, &OutputInfo)> {
        self.active().or_else(|| self.primary())
    }

    pub fn active_or_primary_handle(&self) -> Option<OutputHandle> {
        self.active_output.or(self.primary_output)
    }

    pub fn set_active(&mut self, handle: Option<OutputHandle>) {
        if let Some(h) = handle {
            if self.outputs.contains_key(&h) {
                self.active_output = Some(h);
            }
        } else {
            self.active_output = None;
        }
    }

    pub fn set_primary(&mut self, handle: OutputHandle) -> bool {
        if !self.outputs.contains_key(&handle) {
            return false;
        }

        if let Some(old_primary) = self.primary_output {
            if let Some(old_info) = self.outputs.get_mut(&old_primary) {
                old_info.set_primary(false);
            }
        }

        self.primary_output = Some(handle);

        if let Some(new_info) = self.outputs.get_mut(&handle) {
            new_info.set_primary(true);
        }

        true
    }

    pub fn select_by_policy(&self, policy: &OutputPolicy) -> Vec<(OutputHandle, &OutputInfo)> {
        self.outputs
            .iter()
            .filter(|(_, info)| policy.should_render(info))
            .map(|(handle, info)| (*handle, info))
            .collect()
    }

    pub fn count(&self) -> usize {
        self.outputs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.outputs.is_empty()
    }
}

impl Default for OutputRegistry {
    fn default() -> Self {
        Self::new()
    }
}
