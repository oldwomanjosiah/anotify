use std::{
    path::PathBuf,
    sync::{atomic::AtomicUsize, Arc},
};

use crate::{
    bridge::{CollectorRequest, CollectorRx, Request, RequestRx, RequestTx},
    EventFilter,
};

/// public unique id for watch
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Id(usize);

#[derive(Debug)]
pub(crate) struct SharedState {
    next_id: AtomicUsize,
    channel_size: usize,
    requests: RequestTx,
}

impl SharedState {
    pub const DEFAULT_CAPACITY: usize = 32;

    pub fn new() -> (Arc<Self>, RequestRx) {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }

    pub fn with_capacity(channel_size: usize) -> (Arc<Self>, RequestRx) {
        let (requests, rx) = tokio::sync::mpsc::channel(channel_size);

        let shared = Self {
            next_id: 0.into(),
            channel_size,
            requests,
        };

        (Arc::new(shared), rx)
    }

    pub fn next_id(&self) -> Id {
        use std::sync::atomic::Ordering;

        Id(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    pub async fn request(
        &self,
        once: bool,
        path: PathBuf,
        filter: EventFilter,
    ) -> Option<(Id, CollectorRx)> {
        let (sender, rx) = tokio::sync::mpsc::channel(self.channel_size);
        let id = self.next_id();

        let req = CollectorRequest {
            id,
            path,
            once,
            sender,
            filter,
        };

        if self.requests.send(Request::Create(req)).await.is_ok() {
            Some((id, rx))
        } else {
            None
        }
    }

    pub fn on_drop(&self, id: Id) {
        if self.requests.try_send(Request::Drop(id)).is_err() {
            tracing_impl::info!("Could not notify task of drop");
        }
    }

    pub fn send_close(&self) -> bool {
        self.requests.try_send(Request::Close).is_ok()
    }
}

pub(crate) type Shared = Arc<SharedState>;
