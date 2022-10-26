use std::path::{Path, PathBuf};

use enumflags2::BitFlags;

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

#[derive(Debug)]
pub enum AnotifyErrorType {
    DoesNotExist,
    ExpectedDir,
    ExpectedFile,
    FileRemoved,
    SystemResourceLimit,
    NoPermission,
    InvalidFilePath,
    Unknown {
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

#[derive(Debug)]
pub struct AnotifyError {
    pub(crate) message: Option<String>,
    // backtrace: Option<Backtrace>,
    pub(crate) path: Option<PathBuf>,
    pub(crate) ty: AnotifyErrorType,
}

impl AnotifyError {
    pub(crate) fn new(ty: AnotifyErrorType) -> Self {
        Self {
            message: None,
            path: None,
            ty,
        }
    }

    pub(crate) fn attach_path(&mut self, path: impl Into<PathBuf>) -> &mut Self {
        self.path.replace(path.into());
        self
    }

    pub(crate) fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path.replace(path.into());
        self
    }

    pub(crate) fn attach_message(&mut self, message: impl Into<String>) -> &mut Self {
        self.message.replace(message.into());
        self
    }

    pub(crate) fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message.replace(message.into());
        self
    }
}

pub type Result<T, E = AnotifyError> = core::result::Result<T, E>;

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
