use std::path::PathBuf;

use tokio::task::JoinHandle;

use crate::errors::{AnotifyError, AnotifyErrorType, Result};
use crate::events::EventFilter;
use crate::futures::{self, AnotifyFuture, AnotifyStream};
use crate::shared::Shared;

/// Anotify Instance
#[derive(Debug)]
pub struct Anotify {
    pub(crate) cancel_on_drop: bool,
    pub(crate) inner: AnotifyHandle,
    pub(crate) jh: JoinHandle<()>,
}

/// Non-Owning Handle Anotify Instance
#[derive(Clone, Debug)]
pub struct AnotifyHandle {
    pub(crate) shared: Shared,
}

impl Anotify {
    pub fn builder() -> super::builder::AnotifyBuilder<super::Platform> {
        super::builder::AnotifyBuilder::new()
    }

    pub fn new() -> Result<Self> {
        Self::builder().build()
    }

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

        self.join().await;
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

        Ok(futures::fut(self.shared.clone(), id, recv))
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

        Ok(futures::stream(self.shared.clone(), id, recv))
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
