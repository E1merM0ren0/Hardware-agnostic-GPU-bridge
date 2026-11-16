#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! Vendor-neutral host-side synchronization primitives.
//!
//! This crate deliberately does not expose Vulkan objects. A later Vulkan adapter
//! will translate device events into calls to `complete` and `fail`, while keeping
//! Vulkan handles private to the GPU backend.

mod coordinator;
mod error;
mod handle;
mod timeline;

pub use coordinator::{SynchronizationCoordinator, TimelineSnapshot};
pub use error::{FailureCode, SyncError};
pub use handle::ResourceHandle;
