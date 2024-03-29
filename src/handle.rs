use nix::sys::inotify::AddWatchFlags;
use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
    path::PathBuf,
    time::Duration,
};
use thiserror::Error;
use tokio::{
    sync::{mpsc::Sender as MpscSend, oneshot::Sender as OnceSend},
    task::JoinHandle,
};
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    futures::{DirectoryWatchFuture, DirectoryWatchStream, FileWatchFuture, FileWatchStream},
    task::WatchRequestInner,
};

#[derive(Debug, Clone)]
pub struct Handle {
    pub(crate) request_tx: MpscSend<WatchRequestInner>,
}

#[derive(Debug)]
pub struct OwnedHandle {
    pub(crate) inner: Handle,
    pub(crate) shutdown: OnceSend<()>,
    pub(crate) join: JoinHandle<()>,
}

impl OwnedHandle {
    pub const DEFAULT_SHUTDOWN: Duration = Duration::from_secs(2);
    pub const DEFAULT_REQUEST_BUFFER: usize = 32;

    pub async fn shutdown_with(mut self, wait: Duration) {
        let _ = self.shutdown.send(());

        let join = tokio::time::timeout(wait, &mut self.join);

        match join.await {
            Err(_) => self.join.abort(),
            Ok(Err(e)) => {
                if e.is_cancelled() {
                    panic!("The Watch Task was cancelled without consuming the OwnedHandle");
                }

                std::panic::resume_unwind(e.into_panic());
            }
            Ok(Ok(())) => {}
        }
    }

    pub async fn shutdown(self) {
        self.shutdown_with(Self::DEFAULT_SHUTDOWN).await
    }

    pub async fn wait(self) -> Result<(), tokio::task::JoinError> {
        self.join.await
    }
}

impl Deref for OwnedHandle {
    type Target = Handle;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for OwnedHandle {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[derive(Debug, Error)]
pub enum RequestError {
    #[error("There is no file or directory at the path: {0}")]
    DoesNotExist(PathBuf),
    #[error("The inode at {0} does not have the correct type for this operation")]
    IncorrectType(PathBuf),
}

#[derive(Debug, Error)]
pub enum WatchError {
    #[error("The watcher task was shutdown while before the next event could be received")]
    WatcherShutdown,
}

impl Handle {
    /// Create a file watch builder
    pub fn file(&mut self, path: PathBuf) -> Result<WatchRequest<'_, FileEvents>, RequestError> {
        if !path.exists() {
            return Err(RequestError::DoesNotExist(path));
        }
        if path.is_dir() {
            return Err(RequestError::IncorrectType(path));
        }

        Ok(WatchRequest {
            handle: self,
            path,
            buffer: FileEvents::DEFAULT_BUFFER,
            flags: AddWatchFlags::empty(),
            _type: Default::default(),
        })
    }

    /// Create a directory watch builder
    pub fn dir(
        &mut self,
        path: PathBuf,
    ) -> Result<WatchRequest<'_, DirectoryEvents>, RequestError> {
        // TODO(josiah) make take Into<Path>

        if !path.exists() {
            return Err(RequestError::DoesNotExist(path));
        }
        if !path.is_dir() {
            return Err(RequestError::IncorrectType(path));
        }

        Ok(WatchRequest {
            handle: self,
            path,
            buffer: DirectoryEvents::DEFAULT_BUFFER,
            flags: AddWatchFlags::empty(),
            _type: Default::default(),
        })
    }
}

mod sealed {
    pub trait Sealed {}
}

pub trait WatchType: sealed::Sealed {
    const DEFAULT_BUFFER: usize;
}

pub enum FileEvents {}
pub enum DirectoryEvents {}

impl sealed::Sealed for FileEvents {}
impl sealed::Sealed for DirectoryEvents {}

impl WatchType for FileEvents {
    const DEFAULT_BUFFER: usize = 16;
}

impl WatchType for DirectoryEvents {
    const DEFAULT_BUFFER: usize = 32;
}

/// Configuration and dispatch for a watch
pub struct WatchRequest<'handle, T: WatchType> {
    handle: &'handle mut Handle,
    path: PathBuf,
    buffer: usize,
    flags: AddWatchFlags,
    _type: PhantomData<T>,
}

/// # Common Configuration Methods
impl<T: WatchType> WatchRequest<'_, T> {
    /// Set the amount of items for this watch to buffer,
    ///
    /// value is not considered for single event watches
    pub fn buffer(mut self, size: usize) -> Self {
        self.buffer = size;
        self
    }

    /// Set weather file read events should be captured
    pub fn read(mut self, set: bool) -> Self {
        self.flags.set(AddWatchFlags::IN_ACCESS, set);
        self
    }

    /// Set weather file open events should be captured
    pub fn modify(mut self, set: bool) -> Self {
        self.flags.set(AddWatchFlags::IN_MODIFY, set);
        self
    }

    /// Set weather file open events should be captured
    pub fn open(mut self, set: bool) -> Self {
        self.flags.set(AddWatchFlags::IN_OPEN, set);
        self
    }

    /// Set weather file close events should be generated
    pub fn close(mut self, set: bool) -> Self {
        self.flags.set(AddWatchFlags::IN_CLOSE, set);
        self
    }

    // TODO(josiah) moves will require a more robust background task so that move events can be
    // coalesced correctly
}

/// # File Specific Dispatch Methods
impl<'handle> WatchRequest<'handle, FileEvents> {
    /// Create a watch which will only return the next captured event, and then unsubscribe
    ///
    /// Ignores the value set by [`buffer`][`WatchRequest::buffer`]
    pub async fn next(self) -> Result<FileWatchFuture, WatchError> {
        let (sender, rx) = tokio::sync::oneshot::channel();

        let sender = crate::task::Sender::Once(sender);

        let (setup_tx, setup_rx) = tokio::sync::oneshot::channel();

        self.handle
            .request_tx
            .try_send(WatchRequestInner::Start {
                flags: self.flags,
                path: self.path,
                dir: false,
                sender,
                watch_token_tx: setup_tx,
            })
            .map_err(|_| WatchError::WatcherShutdown)?;

        let watch_token = setup_rx.await.map_err(|_| WatchError::WatcherShutdown)?;

        Ok(FileWatchFuture {
            inner: rx,
            watch_token,
            closed: false,
            handle: self.handle.clone(),
        })
    }

    /// Create a watch which will capture and return a stream of events until dropped.
    ///
    /// Will keep oldest events on buffer overflow set by [`buffer`][`WatchRequest::buffer`]
    pub async fn watch(self) -> Result<FileWatchStream, WatchError> {
        let (sender, rx) = tokio::sync::mpsc::channel(self.buffer);

        let sender = crate::task::Sender::Stream(sender);

        let (setup_tx, setup_rx) = tokio::sync::oneshot::channel();

        self.handle
            .request_tx
            .try_send(WatchRequestInner::Start {
                flags: self.flags,
                path: self.path,
                dir: false,
                sender,
                watch_token_tx: setup_tx,
            })
            .map_err(|_| WatchError::WatcherShutdown)?;

        let watch_token = setup_rx.await.map_err(|_| WatchError::WatcherShutdown)?;

        Ok(FileWatchStream {
            inner: ReceiverStream::from(rx),
            watch_token,
            handle: self.handle.clone(),
        })
    }
}

/// # Directory Specific Dispatch Methods
impl<'handle> WatchRequest<'handle, DirectoryEvents> {
    /// Create a watch which will only return the next captured event, and then unsubscribe
    ///
    /// Ignores the value set by [`buffer`][`WatchRequest::buffer`]
    pub async fn next(self) -> Result<DirectoryWatchFuture, WatchError> {
        let (sender, rx) = tokio::sync::oneshot::channel();

        let sender = crate::task::Sender::Once(sender);

        let (setup_tx, setup_rx) = tokio::sync::oneshot::channel();

        self.handle
            .request_tx
            .try_send(WatchRequestInner::Start {
                flags: self.flags,
                path: self.path,
                dir: true,
                sender,
                watch_token_tx: setup_tx,
            })
            .map_err(|_| WatchError::WatcherShutdown)?;

        let watch_token = setup_rx.await.map_err(|_| WatchError::WatcherShutdown)?;

        Ok(DirectoryWatchFuture {
            inner: rx,
            watch_token,
            handle: self.handle.clone(),
            closed: false,
        })
    }

    /// Create a watch which will capture and return a stream of events until dropped.
    ///
    /// Will keep oldest events on buffer overflow set by [`buffer`][`WatchRequest::buffer`]
    pub async fn watch(self) -> Result<DirectoryWatchStream, WatchError> {
        let (sender, rx) = tokio::sync::mpsc::channel(self.buffer);

        let sender = crate::task::Sender::Stream(sender);

        let (setup_tx, setup_rx) = tokio::sync::oneshot::channel();

        self.handle
            .request_tx
            .try_send(WatchRequestInner::Start {
                flags: self.flags,
                path: self.path,
                dir: true,
                sender,
                watch_token_tx: setup_tx,
            })
            .map_err(|_| WatchError::WatcherShutdown)?;

        let watch_token = setup_rx.await.map_err(|_| WatchError::WatcherShutdown)?;

        Ok(DirectoryWatchStream {
            inner: ReceiverStream::from(rx),
            watch_token,
            handle: self.handle.clone(),
        })
    }
}
