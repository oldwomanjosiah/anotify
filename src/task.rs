use crate::{
    binding::{Binding, BindingEvent},
    bridge::{Request, RequestRx},
    errors::Result,
    registry::Registry,
    shared::Shared,
};

use std::{fmt::Debug, hash::Hash};

pub(crate) struct TaskState<B, I> {
    root_span: tracing::Span,
    #[allow(unused)]
    shared: Shared,
    requests: RequestRx,
    registry: Registry<I>,
    binding: B,
}

impl<B, I> TaskState<B, I> {
    pub fn new(shared: Shared, requests: RequestRx, binding: B) -> Result<Self> {
        let root_span = tracing::info_span!("anotify_task");

        root_span.in_scope(|| tracing::info!("Created"));

        Ok(Self {
            root_span,
            binding,
            requests,
            registry: Registry::new(),
            shared,
        })
    }

    #[allow(unused)]
    fn get_shared(&self) -> Shared {
        self.shared.clone()
    }
}

impl<B, I> TaskState<B, I>
where
    B: Binding<Identifier = I> + Send + 'static,
    I: Copy + Eq + Hash + Debug,
    I: Send + 'static,
{
    pub fn launch_in(self, runtime: tokio::runtime::Handle) -> tokio::task::JoinHandle<()> {
        let _guard = runtime.enter();

        let span = self.root_span.clone();

        tokio::spawn(tracing::Instrument::instrument(self.worker(), span))
    }
}

impl<B, I> TaskState<B, I>
where
    B: Binding<Identifier = I>,
    I: Copy + Eq + Hash + Debug,
{
    fn handle_request(&mut self, request: Request) -> Result<bool> {
        match request {
            Request::Create(request) => {
                self.registry
                    .register_interest(&mut self.binding, request)?;
            }

            Request::Drop(id) => {
                self.registry.deregister_interest(&mut self.binding, id)?;
            }

            Request::Close => return Ok(false),
        }

        Ok(true)
    }

    fn handle_events(&mut self, events: Vec<BindingEvent<I>>) -> Result<()> {
        let to_remove = self.registry.try_send_events(events)?;

        for id in to_remove.into_iter() {
            self.registry.deregister_interest(&mut self.binding, id)?;
        }

        Ok(())
    }

    async fn worker(mut self)
    where
        B: Binding<Identifier = I>,
    {
        tracing::info!("Starting");

        let mut requests_closed: bool = false;

        loop {
            tokio::select! {
                req = self.requests.recv(), if !requests_closed => {
                    let Some(req) = req else {
                        tracing::info!("Requests channel was closed");
                        requests_closed = true;
                        continue;
                    };

                    match self.handle_request(req) {
                        Err(e) => {
                            tracing::error!("While Handling Request:\n{e}");
                        }
                        Ok(true) => {},
                        Ok(false) => {
                            tracing::info!("Close Requested");
                            break;
                        }
                    }
                },
                events = self.binding.events(), if !self.registry.empty() => {
                    match events {
                        Ok(events) => if let Err(e) = self.handle_events(events) {
                            // TODO Should these be fatal?
                            tracing::error!("While handling Events:\n{e}");
                        }
                        Err(e) => {
                            tracing::error!("While getting Events:\n{e}");
                            break;
                        }
                    }
                },
                else => {
                    tracing::info!("Requests Closed, Registry Empty");
                    break;
                }
            }
        }

        tracing::info!("Exiting");
    }
}
