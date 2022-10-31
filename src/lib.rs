pub mod binding;
pub mod errors;
pub mod inotify;

pub mod builder;

pub fn builder() -> builder::AnotifyBuilder<Platform> {
    builder::AnotifyBuilder::new()
}

// To Re-Export
mod events;
mod futures;
mod handle;

pub use events::*;
pub use handle::*;

// Internals

/// Types used to communicate between the internal task and user-facing
/// types.
mod bridge;

/// Registry for keeping track of watches, separate from API binding implementation.
mod registry;

/// State which is shared between the task, handles, and futures.
mod shared;

/// Task implementation
mod task;

/// Default Platform Bindings which will be used.
pub type Platform = inotify::InotifyBinding;
