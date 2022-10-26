use tracing_impl::Instrument;

use crate::new::Result;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize},
    Arc,
};

mod binding;
mod inotify;

pub type Platform = inotify::InotifyBinding;

/// public unique id for watch
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Id(usize);

pub(crate) struct SharedState {
    next_id: AtomicUsize,
}

impl SharedState {
    pub fn next_id(&self) -> Id {
        use std::sync::atomic::Ordering;

        Id(self.next_id.fetch_add(1, Ordering::Relaxed))
    }
}

type Shared = Arc<SharedState>;

impl SharedState {
    fn new() -> Arc<Self> {
        let inner = SharedState { next_id: 0.into() };

        Arc::new(inner)
    }
}

struct TaskState<B> {
    root_span: tracing_impl::Span,
    shared: Shared,
    binding: B,
}

impl<B> TaskState<B> {
    fn new() -> Result<TaskState<Platform>> {
        TaskState::new_with(Platform::new()?)
    }

    fn new_with(binding: B) -> Result<Self> {
        let root_span = tracing_impl::info_span!("anotify_task");

        let shared = SharedState::new();

        root_span.in_scope(|| tracing_impl::info!("Created"));

        Ok(Self {
            root_span,
            binding,
            shared,
        })
    }

    fn get_shared(&self) -> Shared {
        self.shared.clone()
    }

    fn launch_in(self, runtime: tokio::runtime::Handle) -> tokio::task::JoinHandle<()>
    where
        B: binding::Binding + Send + 'static,
    {
        let _guard = runtime.enter();

        let span = self.root_span.clone();

        tokio::spawn(
            async move {
                self.worker().await;
            }
            .instrument(span),
        )
    }

    async fn worker(self)
    where
        B: binding::Binding,
    {
        tracing_impl::info!("Starting");
        todo!();
        tracing_impl::info!("Exiting");
    }
}
