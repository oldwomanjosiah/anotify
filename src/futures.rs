use crate::new::internal::{Id, Shared};

use super::{super::internal::bridge::CollectorRx, error::Result, Event};

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
