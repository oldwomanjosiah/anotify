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

mod fut {
    use super::{super::internal::bridge::CollectorRx, error::Result, Event};

    struct AnotifyFutInternal {
        shared: crate::new::internal::Shared,
        id: crate::new::internal::Id,
        recv: CollectorRx,
    }

    impl Drop for AnotifyFutInternal {
        fn drop(&mut self) {
            self.shared.on_drop(self.id);
        }
    }

    pub struct AnotifyFuture {
        internal: Option<AnotifyFutInternal>,
    }

    pub struct AnotifyStream {
        internal: AnotifyFutInternal,
    }

    impl std::future::Future for AnotifyFuture {
        type Output = Result<Event>;

        fn poll(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            use std::task::*;

            fn closed(message: &'static str) -> Result<Event> {
                use super::error::*;
                Err(AnotifyError::new(AnotifyErrorType::Closed).with_message(message))
            }

            let Some(int) = &mut self.internal else {
                return Poll::Ready(closed("Polled after completion"));
            };

            let inner = match ready!(int.recv.poll_recv(cx)) {
                Some(it) => {
                    self.internal = None;

                    it
                }
                None => closed("Before First Read"),
            };

            Poll::Ready(inner)
        }
    }

    impl tokio_stream::Stream for AnotifyStream {
        type Item = Result<Event>;

        fn poll_next(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            self.internal.recv.poll_recv(cx)
        }
    }
}

/// Common methods for all anotify instance handles
trait AnotifyHandle {
    fn wait(
        &self,
        path: impl AsRef<Path>,
        filter: impl Into<EventFilter>,
    ) -> Result<fut::AnotifyFuture>;

    fn stream(
        &self,
        path: impl AsRef<Path>,
        filter: impl Into<EventFilter>,
    ) -> Result<fut::AnotifyStream>;
}
