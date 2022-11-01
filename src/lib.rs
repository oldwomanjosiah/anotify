#![doc = include_str!("docs/lib.md")]

/// Common Platform API Binding Interface
pub mod binding;

/// Errors produced by this crate
pub mod errors;

/// Platform API Binding Interface for the iNotify Linux API
pub mod inotify;

// To Re-Export
mod builder;
mod events;
mod futures;
mod handle;

pub use builder::*;
pub use events::*;
pub use futures::*;
pub use handle::*;

/// Default Platform Bindings which will be used.
pub type Platform = inotify::InotifyBinding;

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
