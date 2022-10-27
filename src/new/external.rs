use std::path::{Path, PathBuf};

use enumflags2::BitFlags;

use self::error::Result;

pub mod error;

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

#[repr(u16)]
#[enumflags2::bitflags(default = Write)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventFilterType {
    Read,
    Write,
    Open,
    CloseNoModify,
    CloseModify,
    Move,
    Metadata,
    Create,
    Delete,
    DirOnly,
    FileOnly,
}

pub type EventFilter = BitFlags<EventFilterType>;

struct AnotifyFuture {}

struct AnotifyStream {}

/// Common methods for all anotify instance handles
trait AnotifyHandle {
    fn wait(&self, path: impl AsRef<Path>, filter: impl Into<EventFilter>)
        -> Result<AnotifyFuture>;

    fn stream(
        &self,
        path: impl AsRef<Path>,
        filter: impl Into<EventFilter>,
    ) -> Result<AnotifyStream>;
}

fn test<T: AnotifyHandle>(item: T) {
    let event1 = item.stream("readme.md", EventFilterType::Write).unwrap();
    let event2 = item
        .stream("readme.md", EventFilterType::Write | EventFilterType::Read)
        .unwrap();
}
