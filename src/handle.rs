use core::fmt;

/// Opaque identity for a resource owned by a registry.
///
/// The generation changes whenever a slot is recycled. Callers must never
/// construct handles by guessing indices or generations.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceHandle {
    index: u32,
    generation: u32,
}

impl ResourceHandle {
    pub(crate) const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// Returns the slot index for diagnostics only.
    pub const fn index(self) -> u32 {
        self.index
    }

    /// Returns the generation for diagnostics only.
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

impl fmt::Display for ResourceHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.index, self.generation)
    }
}
