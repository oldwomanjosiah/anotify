use std::path::{Path, PathBuf};

use enumflags2::BitFlags;

use self::error::Result;

pub mod error;
pub mod fut;

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

mod builder {
    use std::hash::Hash;

    use crate::new::external::Result;

    use super::handle::Anotify;

    pub struct Builder<B> {
        buffer: usize,
        handle: Option<tokio::runtime::Handle>,
        _phantom: std::marker::PhantomData<B>,
    }

    impl<B> Builder<B> {
        pub fn new() -> Builder<crate::new::internal::inotify::InotifyBinding> {
            Builder {
                buffer: crate::new::internal::SharedState::DEFAULT_CAPACITY,
                handle: None,
                _phantom: Default::default(),
            }
        }

        pub fn with_runtime(mut self, handle: tokio::runtime::Handle) -> Self {
            Self {
                handle: Some(handle),
                ..self
            }
        }

        pub fn with_buffer(mut self, buffer: usize) -> Self {
            Self { buffer, ..self }
        }

        pub fn build(self) -> Result<Anotify>
        where
            B: crate::new::internal::binding::Binding,
            B::Identifier: std::fmt::Debug + Eq + Hash + Send + 'static,
            B: Send,
        {
            let (shared, requests) = crate::new::internal::SharedState::with_capacity(self.buffer);

            let binding = B::new()?;

            let task_state =
                crate::new::internal::TaskState::new_with(shared.clone(), requests, binding)?;

            let handle = self
                .handle
                .unwrap_or_else(|| tokio::runtime::Handle::current());

            let jh = task_state.launch_in(handle);

            let inner = super::handle::AnotifyHandle { shared };

            Ok(Anotify {
                cancel_on_drop: false,
                inner,
                jh,
            })
        }
    }
}

mod handle {
    use std::path::PathBuf;

    use tokio::task::JoinHandle;

    use crate::new::error::Result;

    use super::{
        super::internal::Shared,
        error::{AnotifyError, AnotifyErrorType},
        fut::{AnotifyFuture, AnotifyStream},
        EventFilter,
    };

    #[derive(Debug)]
    pub struct Anotify {
        cancel_on_drop: bool,
        inner: AnotifyHandle,
        jh: JoinHandle<()>,
    }

    #[derive(Clone, Debug)]
    pub struct AnotifyHandle {
        shared: Shared,
    }

    impl Anotify {
        /// Wait for this `Anotify` instance to close on it's own.
        pub async fn join(mut self) {
            let Err(e) = (&mut self.jh).await else { return; };

            if e.is_panic() {
                std::panic::resume_unwind(e.into_panic());
            } else {
                tracing_impl::error!(error = %e, "Could not join on task");
            }
        }

        /// Try to close this instance, if it is still running.
        /// Returns whether this action closed the instance.
        pub async fn close(self) -> bool {
            if !self.shared.send_close() {
                return false;
            }

            self.join().await;

            true
        }

        /// Abort the tokio task associated with this instance.
        pub async fn abort(self) {
            if self.jh.is_finished() {
                return;
            }

            self.jh.abort();

            self.join();
        }

        /// Downgrade this owned handle into an unprivledged handle without
        /// cancelling the worker task (avoiding the cancel on drop).
        /// This is a one way operation.
        pub fn downgrade(mut self) -> AnotifyHandle {
            self.cancel_on_drop = false;
            self.handle()
        }

        /// Get an unprivledged handle to the Anotify Instance.
        pub fn handle(&self) -> AnotifyHandle {
            self.inner.clone()
        }
    }

    impl AnotifyHandle {
        pub async fn next(
            &self,
            path: impl Into<PathBuf> + Clone,
            filter: impl Into<EventFilter>,
        ) -> Result<AnotifyFuture> {
            let Some((id, recv)) = self
                .shared
                .request(true, path.clone().into(), filter.into())
                .await else {
                    return Err(AnotifyError::new(AnotifyErrorType::Closed).with_path(path.into()))
            };

            Ok(super::fut::fut(self.shared.clone(), id, recv))
        }

        pub async fn watch(
            &self,
            path: impl Into<PathBuf> + Clone,
            filter: impl Into<EventFilter>,
        ) -> Result<AnotifyStream> {
            let Some((id, recv)) = self
                .shared
                .request(true, path.clone().into(), filter.into())
                .await else {
                    return Err(AnotifyError::new(AnotifyErrorType::Closed).with_path(path.into()))
            };

            Ok(super::fut::stream(self.shared.clone(), id, recv))
        }
    }

    impl std::ops::Deref for Anotify {
        type Target = AnotifyHandle;

        fn deref(&self) -> &Self::Target {
            &self.inner
        }
    }

    // Implementing drop makes the code around joining a lot more
    // annoying, since there is no way to disassemble the type
    // (it would avoid the drop check). If this functionality is desired
    impl std::ops::Drop for Anotify {
        fn drop(&mut self) {
            if self.cancel_on_drop {
                self.inner.shared.send_close();
            }
        }
    }
}
