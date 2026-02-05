use std::collections::HashMap;

use crate::value_objects::handle::PopupHandle;

#[derive(Debug, Default, Clone)]
pub struct PopupTree {
    root_popups: Vec<PopupHandle>,
    relationships: HashMap<PopupHandle, PopupNode>,
}

#[derive(Debug, Clone)]
pub struct PopupNode {
    parent: Option<PopupHandle>,
    children: Vec<PopupHandle>,
    depth: usize,
}

impl PopupTree {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_popup(&mut self, handle: PopupHandle, parent: Option<PopupHandle>) {
        if self.relationships.contains_key(&handle) {
            return;
        }

        let depth = parent
            .and_then(|p| self.relationships.get(&p).map(|n| n.depth + 1))
            .unwrap_or(0);

        self.relationships.insert(
            handle,
            PopupNode {
                parent,
                children: Vec::new(),
                depth,
            },
        );

        if let Some(parent) = parent {
            if let Some(parent_node) = self.relationships.get_mut(&parent) {
                parent_node.children.push(handle);
            }
        } else {
            self.root_popups.push(handle);
        }
    }

    /// Removes a popup and returns the handles that should be cascade-removed.
    ///
    /// The returned list contains `handle` and all descendants in a stable order
    /// (parents before children).
    pub fn remove_popup(&mut self, handle: PopupHandle) -> Vec<PopupHandle> {
        let mut cascade = Vec::new();
        self.collect_descendants(handle, &mut cascade);

        if let Some(node) = self.relationships.remove(&handle) {
            if let Some(parent) = node.parent {
                if let Some(parent_node) = self.relationships.get_mut(&parent) {
                    parent_node.children.retain(|&c| c != handle);
                }
            } else {
                self.root_popups.retain(|&h| h != handle);
            }
        }

        for removed in cascade.iter().copied().filter(|&h| h != handle) {
            if let Some(node) = self.relationships.remove(&removed) {
                if let Some(parent) = node.parent {
                    if let Some(parent_node) = self.relationships.get_mut(&parent) {
                        parent_node.children.retain(|&c| c != removed);
                    }
                } else {
                    self.root_popups.retain(|&h| h != removed);
                }
            }
        }

        cascade
    }

    #[must_use]
    pub fn get_children(&self, handle: PopupHandle) -> &[PopupHandle] {
        self.relationships
            .get(&handle)
            .map_or(&[], |n| n.children.as_slice())
    }

    #[must_use]
    pub fn get_parent(&self, handle: PopupHandle) -> Option<PopupHandle> {
        self.relationships.get(&handle).and_then(|n| n.parent)
    }

    #[must_use]
    pub fn get_ancestors(&self, mut handle: PopupHandle) -> Vec<PopupHandle> {
        let mut ancestors = Vec::new();
        while let Some(parent) = self.get_parent(handle) {
            ancestors.push(parent);
            handle = parent;
        }
        ancestors
    }

    #[must_use]
    pub fn is_descendant(&self, mut child: PopupHandle, ancestor: PopupHandle) -> bool {
        while let Some(parent) = self.get_parent(child) {
            if parent == ancestor {
                return true;
            }
            child = parent;
        }
        false
    }

    #[must_use]
    pub fn roots(&self) -> &[PopupHandle] {
        &self.root_popups
    }

    fn collect_descendants(&self, handle: PopupHandle, out: &mut Vec<PopupHandle>) {
        out.push(handle);
        for &child in self.get_children(handle) {
            self.collect_descendants(child, out);
        }
    }
}
