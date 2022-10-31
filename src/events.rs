use std::path::{Path, PathBuf};

use enumflags2::BitFlags;

use self::error::Result;

/// The type of event captured by this watch
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Hash)]
pub enum EventType {
    Read,
    Write,
    Open,
    Close { modified: bool },
    Move { to: Option<PathBuf> },
    Create,
    Delete,
    Metadata,
}

/// Events returned from a watch
#[derive(Debug, Clone, PartialEq, Hash)]
pub struct Event {
    pub path: PathBuf,
    pub ty: EventType,
}

impl Event {
    /// Checks if a given filter contains this event.
    pub(crate) fn contained_in(&self, filter: &EventFilter) -> bool {
        let as_filter = match &self.ty {
            EventType::Read => EventFilterType::Read.into(),
            EventType::Write => EventFilterType::Write.into(),
            EventType::Open => EventFilterType::Open.into(),
            EventType::Close { modified: true } => EventFilterType::CloseModify,
            EventType::Close { modified: false } => EventFilterType::CloseNoModify,
            EventType::Move { .. } => EventFilterType::Move,
            EventType::Create => EventFilterType::Create,
            EventType::Delete => EventFilterType::Delete,
            EventType::Metadata => EventFilterType::Metadata,
        };

        filter.contains(as_filter)
    }
}

/// Filters for creating a new watch.
#[repr(u16)]
#[enumflags2::bitflags(default = Write | CloseModify)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventFilterType {
    /// File or directory was read
    Read,
    /// File or directory was written/modified
    Write,
    /// File or directory was opened
    Open,
    /// File or directory was closed, but was not
    /// modifiable while open
    CloseNoModify,
    /// File or directory was closed, and was modifiable
    /// while open
    CloseModify,
    /// File within the watch directory was moved
    Move,
    /// File metadat was updated
    Metadata,
    /// New File was created within the watched directory
    Create,
    /// File was deleted
    Delete,
    /// Only create the watch if the path referes
    /// to a directory. May not be used with [`FileOnly`]
    DirOnly,
    /// Only create the watch if the path referes
    /// to a file. May not be used with [`DirOnly`]
    FileOnly,
}

/// Combined filter flags.
pub type EventFilter = BitFlags<EventFilterType>;
