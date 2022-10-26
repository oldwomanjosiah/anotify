use std::path::{Path, PathBuf};

use enumflags2::BitFlags;

use self::error::Result;

pub mod error;

pub enum EventType {
    Read,
    Write,
    Open,
    Close { modified: bool },
    Move { to: Option<PathBuf> },
    Delete,
    // TODO(josiah) should this contain the new metadata?
    Metadata,
}

pub struct Event {
    path: PathBuf,
    ty: EventType,
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
