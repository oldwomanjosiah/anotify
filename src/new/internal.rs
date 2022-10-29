use tracing_impl::{callsite::Identifier, Instrument};

use crate::new::error::Result;
use std::{
    collections::HashMap,
    fmt::Debug,
    hash::Hash,
    path::PathBuf,
    sync::{atomic::AtomicUsize, Arc},
};

use super::EventFilter;

mod binding;
mod inotify;
mod registry;

mod bridge {
    use crate::new::{error::Result, external::Event, internal::Id, EventFilter};
    use std::path::PathBuf;
    use tokio::sync::mpsc::{Receiver, Sender};

    pub type CollectorTx = Sender<Result<Event>>;
    pub type CollectorRx = Receiver<Result<Event>>;

    pub type RequestTx = Sender<Request>;
    pub type RequestRx = Receiver<Request>;

    pub struct CollectorRequest {
        pub id: Id,
        pub path: PathBuf,
        pub once: bool,
        pub sender: CollectorTx,
        pub filter: EventFilter,
    }

    pub enum Request {
        Create(CollectorRequest),
        Drop(Id),
    }
}

pub type Platform = inotify::InotifyBinding;

/// public unique id for watch
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Id(usize);

pub(crate) struct SharedState {
    next_id: AtomicUsize,
    channel_size: usize,
    requests: bridge::RequestTx,
}

impl SharedState {
    const DEFAULT_CAPACITY: usize = 32;

    pub fn new() -> (Arc<Self>, bridge::RequestRx) {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }

    pub fn with_capacity(channel_size: usize) -> (Arc<Self>, bridge::RequestRx) {
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
    ) -> Option<(Id, bridge::CollectorRx)> {
        let (sender, rx) = tokio::sync::mpsc::channel(self.channel_size);
        let id = self.next_id();

        let req = bridge::CollectorRequest {
            id,
            path,
            once,
            sender,
            filter,
        };

        if self
            .requests
            .send(bridge::Request::Create(req))
            .await
            .is_ok()
        {
            Some((id, rx))
        } else {
            None
        }
    }
}

type Shared = Arc<SharedState>;

struct TaskState<B, I> {
    root_span: tracing_impl::Span,
    shared: Shared,
    requests: bridge::RequestRx,
    registry: registry::Registry<I>,
    binding: B,
}

impl<B, I> TaskState<B, I> {
    fn new(
        shared: Shared,
        requests: bridge::RequestRx,
    ) -> Result<TaskState<Platform, <Platform as binding::Binding>::Identifier>> {
        TaskState::new_with(shared, requests, Platform::new()?)
    }

    fn new_with(shared: Shared, requests: bridge::RequestRx, binding: B) -> Result<Self> {
        let root_span = tracing_impl::info_span!("anotify_task");

        root_span.in_scope(|| tracing_impl::info!("Created"));

        Ok(Self {
            root_span,
            binding,
            requests,
            registry: registry::Registry::new(),
            shared,
        })
    }

    fn get_shared(&self) -> Shared {
        self.shared.clone()
    }
}

impl<B, I> TaskState<B, I>
where
    B: binding::Binding<Identifier = I> + Send + 'static,
    I: Copy + Eq + Hash + Debug,
    I: Send + 'static,
{
    fn launch_in(self, runtime: tokio::runtime::Handle) -> tokio::task::JoinHandle<()> {
        let _guard = runtime.enter();

        let span = self.root_span.clone();

        tokio::spawn(
            async move {
                self.worker().await;
            }
            .instrument(span),
        )
    }
}

impl<B, I> TaskState<B, I>
where
    B: binding::Binding<Identifier = I>,
    I: Copy + Eq + Hash + Debug,
{
    fn deregister_all(&mut self, idents: Vec<Id>) -> crate::new::error::Result<()> {
        for id in idents.into_iter() {
            self.registry.deregister_interest(&mut self.binding, id)?;
        }

        Ok(())
    }

    fn handle_request(&mut self, request: bridge::Request) -> crate::new::error::Result<()> {
        match request {
            bridge::Request::Create(request) => {
                self.registry
                    .register_interest(&mut self.binding, request)?;
            }

            bridge::Request::Drop(id) => {
                self.registry.deregister_interest(&mut self.binding, id)?;
            }
        }

        Ok(())
    }

    fn handle_events(
        &mut self,
        events: Vec<binding::BindingEvent<I>>,
    ) -> crate::new::error::Result<()> {
        let to_remove = self.registry.try_send_events(events)?;

        for id in to_remove.into_iter() {
            self.registry.deregister_interest(&mut self.binding, id)?;
        }

        Ok(())
    }

    async fn worker(mut self)
    where
        B: binding::Binding<Identifier = I>,
    {
        tracing_impl::info!("Starting");

        let mut requests_closed: bool = false;

        loop {
            tokio::select! {
                req = self.requests.recv(), if !requests_closed => {
                    let Some(req) = req else {
                        tracing_impl::info!("Requests channel was closed");
                        requests_closed = true;
                        continue;
                    };

                    if let Err(e) = self.handle_request(req) {
                        // TODO Should these be fatal?
                        tracing_impl::error!("While Handling Request:\n{e}");
                    }
                },
                events = self.binding.events(), if !self.registry.empty() => {
                    match events {
                        Ok(events) => if let Err(e) = self.handle_events(events) {
                            // TODO Should these be fatal?
                            tracing_impl::error!("While handling Events:\n{e}");
                        }
                        Err(e) => {
                            tracing_impl::error!("While getting Events:\n{e}");
                            break;
                        }
                    }
                },
                else => {
                    tracing_impl::info!("Requests Closed, Registry Empty");
                    break;
                }
            }
        }

        tracing_impl::info!("Exiting");
    }
}
