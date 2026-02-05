use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_HANDLE_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandleId(usize);

impl HandleId {
    fn new() -> Self {
        Self(NEXT_HANDLE_ID.fetch_add(1, Ordering::Relaxed))
    }

    const fn from_raw(id: usize) -> Self {
        Self(id)
    }

    pub const fn as_usize(&self) -> usize {
        self.0
    }
}

impl Hash for HandleId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Output;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Popup;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Surface;

/// Type-safe unique identifier for runtime resources
///
/// Used as `OutputHandle`, `PopupHandle`, or `SurfaceHandle` to identify
/// specific instances of those resources.
pub struct Handle<T> {
    id: HandleId,
    _marker: PhantomData<T>,
}

impl<T> Debug for Handle<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("Handle").field("id", &self.id).finish()
    }
}

impl<T> Handle<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: HandleId::new(),
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub const fn from_raw(id: usize) -> Self {
        Self {
            id: HandleId::from_raw(id),
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub const fn id(&self) -> usize {
        self.id.as_usize()
    }
}

impl<T> Default for Handle<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(clippy::non_canonical_clone_impl)]
impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            _marker: PhantomData,
        }
    }
}

impl<T> Copy for Handle<T> {}

impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for Handle<T> {}

impl<T> Hash for Handle<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Handle<Popup> {
    #[must_use]
    pub const fn key(self) -> usize {
        self.id()
    }
}

/// Unique identifier for an output (monitor)
pub type OutputHandle = Handle<Output>;
/// Unique identifier for a popup window
pub type PopupHandle = Handle<Popup>;
/// Unique identifier for a layer surface
pub type SurfaceHandle = Handle<Surface>;
