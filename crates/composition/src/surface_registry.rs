use std::collections::HashMap;
use std::rc::Rc;

use layer_shika_adapters::platform::slint_interpreter::ComponentInstance;
use layer_shika_domain::config::SurfaceConfig;
use layer_shika_domain::value_objects::handle::SurfaceHandle;
use layer_shika_domain::value_objects::output_handle::OutputHandle;

use crate::Result;

/// Definition of a surface including component name and configuration
///
/// Pairs a Slint component with its layer-shell settings.
#[derive(Debug, Clone)]
pub struct SurfaceDefinition {
    pub component: String,
    pub config: SurfaceConfig,
}

/// Metadata tracked for each registered surface
///
/// Includes spawn order for deterministic iteration and creation timestamps.
#[derive(Clone, Default)]
pub struct SurfaceMetadata {
    pub spawn_order: usize,
    pub creation_timestamp: u64,
}

/// Registry entry for a surface with handle, name, and output instances
///
/// Tracks all instances of a surface across multiple outputs and maintains
/// the component definition and metadata.
pub struct SurfaceEntry {
    pub handle: SurfaceHandle,
    pub name: String,
    pub component: String,
    pub definition: SurfaceDefinition,
    pub output_instances: HashMap<OutputHandle, Rc<ComponentInstance>>,
    pub metadata: SurfaceMetadata,
}

impl SurfaceEntry {
    /// Creates a new surface entry
    pub fn new(handle: SurfaceHandle, name: String, definition: SurfaceDefinition) -> Self {
        let component = definition.component.clone();
        Self {
            handle,
            name,
            component,
            definition,
            output_instances: HashMap::new(),
            metadata: SurfaceMetadata::default(),
        }
    }

    /// Adds a component instance for a specific output
    pub fn add_output_instance(&mut self, output: OutputHandle, instance: Rc<ComponentInstance>) {
        self.output_instances.insert(output, instance);
    }

    /// Removes and returns a component instance for a specific output
    pub fn remove_output_instance(
        &mut self,
        output: OutputHandle,
    ) -> Option<Rc<ComponentInstance>> {
        self.output_instances.remove(&output)
    }

    /// Returns a component instance for a specific output
    pub fn get_output_instance(&self, output: OutputHandle) -> Option<&Rc<ComponentInstance>> {
        self.output_instances.get(&output)
    }

    /// Returns all output handles for this surface
    pub fn outputs(&self) -> Vec<OutputHandle> {
        self.output_instances.keys().copied().collect()
    }
}

/// Central registry for managing surface entries and lookups
///
/// Maintains indices for efficient lookup by handle, name, or component.
pub struct SurfaceRegistry {
    entries: HashMap<SurfaceHandle, SurfaceEntry>,
    by_name: HashMap<String, Vec<SurfaceHandle>>,
    by_component: HashMap<String, Vec<SurfaceHandle>>,
    next_spawn_order: usize,
}

impl SurfaceRegistry {
    /// Creates a new empty surface registry
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            by_name: HashMap::new(),
            by_component: HashMap::new(),
            next_spawn_order: 0,
        }
    }

    /// Inserts a new surface entry into the registry
    pub fn insert(&mut self, mut entry: SurfaceEntry) -> Result<()> {
        entry.metadata.spawn_order = self.next_spawn_order;
        self.next_spawn_order += 1;

        let handle = entry.handle;
        let name = entry.name.clone();
        let component = entry.component.clone();

        self.by_name.entry(name).or_default().push(handle);

        self.by_component.entry(component).or_default().push(handle);

        self.entries.insert(handle, entry);

        Ok(())
    }

    /// Removes and returns a surface entry by handle
    pub fn remove(&mut self, handle: SurfaceHandle) -> Option<SurfaceEntry> {
        let entry = self.entries.remove(&handle)?;

        if let Some(handles) = self.by_name.get_mut(&entry.name) {
            handles.retain(|&h| h != handle);
            if handles.is_empty() {
                self.by_name.remove(&entry.name);
            }
        }

        if let Some(handles) = self.by_component.get_mut(&entry.component) {
            handles.retain(|&h| h != handle);
            if handles.is_empty() {
                self.by_component.remove(&entry.component);
            }
        }

        Some(entry)
    }

    /// Returns a reference to a surface entry by handle
    pub fn get(&self, handle: SurfaceHandle) -> Option<&SurfaceEntry> {
        self.entries.get(&handle)
    }

    /// Alias for `get`
    pub fn by_handle(&self, handle: SurfaceHandle) -> Option<&SurfaceEntry> {
        self.entries.get(&handle)
    }

    /// Returns a mutable reference to a surface entry by handle
    pub fn get_mut(&mut self, handle: SurfaceHandle) -> Option<&mut SurfaceEntry> {
        self.entries.get_mut(&handle)
    }

    /// Alias for `get_mut`
    pub fn by_handle_mut(&mut self, handle: SurfaceHandle) -> Option<&mut SurfaceEntry> {
        self.entries.get_mut(&handle)
    }

    /// Returns all surface entries with the given name
    pub fn by_name(&self, name: &str) -> Vec<&SurfaceEntry> {
        self.by_name
            .get(name)
            .map(|handles| handles.iter().filter_map(|h| self.entries.get(h)).collect())
            .unwrap_or_default()
    }

    /// Returns mutable references to all surface entries with the given name
    pub fn by_name_mut(&mut self, name: &str) -> Vec<&mut SurfaceEntry> {
        let handles: Vec<SurfaceHandle> = self.by_name.get(name).cloned().unwrap_or_default();

        let entries_ptr = std::ptr::addr_of_mut!(self.entries);

        handles
            .iter()
            .filter_map(|h| unsafe { (*entries_ptr).get_mut(h) })
            .collect()
    }

    /// Returns the first surface handle with the given name
    pub fn handle_by_name(&self, name: &str) -> Option<SurfaceHandle> {
        self.by_name
            .get(name)
            .and_then(|handles| handles.first().copied())
    }

    /// Returns all surface handles with the given name
    pub fn handles_by_name(&self, name: &str) -> Vec<SurfaceHandle> {
        self.by_name.get(name).cloned().unwrap_or_default()
    }

    /// Returns the name for a surface handle
    pub fn name_by_handle(&self, handle: SurfaceHandle) -> Option<&str> {
        self.entries.get(&handle).map(|e| e.name.as_str())
    }

    /// Returns all surface entries for the given component
    pub fn by_component(&self, component: &str) -> Vec<&SurfaceEntry> {
        self.by_component
            .get(component)
            .map(|handles| handles.iter().filter_map(|h| self.entries.get(h)).collect())
            .unwrap_or_default()
    }

    /// Returns an iterator over all surface entries
    pub fn all(&self) -> impl Iterator<Item = &SurfaceEntry> {
        self.entries.values()
    }

    /// Returns an iterator over all mutable surface entries
    pub fn all_mut(&mut self) -> impl Iterator<Item = &mut SurfaceEntry> {
        self.entries.values_mut()
    }

    /// Returns an iterator over all surface handles
    pub fn handles(&self) -> impl Iterator<Item = SurfaceHandle> + '_ {
        self.entries.keys().copied()
    }

    /// Returns all output handles for a surface
    pub fn outputs_for_surface(&self, handle: SurfaceHandle) -> Vec<OutputHandle> {
        self.entries
            .get(&handle)
            .map(SurfaceEntry::outputs)
            .unwrap_or_default()
    }

    /// Returns all surface names in the registry
    pub fn surface_names(&self) -> Vec<&str> {
        self.by_name.keys().map(String::as_str).collect()
    }

    /// Returns all component names in the registry
    pub fn component_names(&self) -> Vec<&str> {
        self.by_component.keys().map(String::as_str).collect()
    }

    /// Returns the number of surfaces in the registry
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Checks if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Checks if a surface handle exists in the registry
    pub fn contains(&self, handle: SurfaceHandle) -> bool {
        self.entries.contains_key(&handle)
    }

    /// Checks if a surface name exists in the registry
    pub fn contains_name(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }
}

impl Default for SurfaceRegistry {
    fn default() -> Self {
        Self::new()
    }
}
