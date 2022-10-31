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

pub mod binding;
pub mod inotify;
pub mod registry;

pub(crate) struct TaskState<B, I> {
    root_span: tracing_impl::Span,
    shared: Shared,
    requests: bridge::RequestRx,
    registry: registry::Registry<I>,
    binding: B,
}

impl<B, I> TaskState<B, I> {
    pub fn new(shared: Shared, requests: bridge::RequestRx, binding: B) -> Result<Self> {
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
    pub fn launch_in(self, runtime: tokio::runtime::Handle) -> tokio::task::JoinHandle<()> {
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

    fn handle_request(&mut self, request: bridge::Request) -> crate::new::error::Result<bool> {
        match request {
            bridge::Request::Create(request) => {
                self.registry
                    .register_interest(&mut self.binding, request)?;
            }

            bridge::Request::Drop(id) => {
                self.registry.deregister_interest(&mut self.binding, id)?;
            }

            bridge::Request::Close => return Ok(false),
        }

        Ok(true)
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

                    match self.handle_request(req) {
                        Err(e) => {
                            tracing_impl::error!("While Handling Request:\n{e}");
                        }
                        Ok(true) => {},
                        Ok(false) => {
                            tracing_impl::info!("Close Requested");
                            break;
                        }
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
