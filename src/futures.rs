use crate::bridge::CollectorRx;
use crate::errors::{AnotifyError, AnotifyErrorType, Result};
use crate::events::Event;
use crate::shared::{Id, Shared};

pub(crate) fn fut(shared: Shared, id: Id, recv: CollectorRx) -> AnotifyFuture {
    let internal = Some(AnotifyFutInternal { shared, id, recv });

    AnotifyFuture { internal }
}

pub(crate) fn stream(shared: Shared, id: Id, recv: CollectorRx) -> AnotifyStream {
    let internal = AnotifyFutInternal { shared, id, recv };

    AnotifyStream { internal }
}

struct AnotifyFutInternal {
    shared: Shared,
    id: Id,
    recv: CollectorRx,
}

impl Drop for AnotifyFutInternal {
    fn drop(&mut self) {
        self.shared.on_drop(self.id);
    }
}

/// Future type for single-event watches [`Anotify::next`][`crate::AnotifyHandle::next`]
pub struct AnotifyFuture {
    internal: Option<AnotifyFutInternal>,
}

/// Stream type for multievent watches [`Anotify::watch`][`crate::AnotifyHandle::watch`]
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
            Err(AnotifyError::new(AnotifyErrorType::Closed).with_message(message))
        }

        let Some(int) = &mut self.internal else {
            return Poll::Ready(closed("Polled after completion"));
        };

        let inner = match ready!(int.recv.poll_recv(cx)) {
            Some(mut it) => {
                self.internal = None;

                if let Err(ref mut e) = it {
                    e.recapture_backtrace();
                }

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
        use std::task::*;
        let mut res = ready!(self.internal.recv.poll_recv(cx));

        if let Some(Err(ref mut e)) = res {
            e.recapture_backtrace();
        }

        Poll::Ready(res)
    }
}
