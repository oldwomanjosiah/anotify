use std::hash::Hash;

use crate::{errors::Result, handle::Anotify, shared::SharedState, task::TaskState};

pub struct AnotifyBuilder<B> {
    buffer: usize,
    handle: Option<tokio::runtime::Handle>,
    _phantom: std::marker::PhantomData<B>,
}

impl<B> AnotifyBuilder<B> {
    pub fn new() -> AnotifyBuilder<B> {
        AnotifyBuilder {
            buffer: SharedState::DEFAULT_CAPACITY,
            handle: None,
            _phantom: Default::default(),
        }
    }

    pub fn with_runtime(self, handle: tokio::runtime::Handle) -> Self {
        Self {
            handle: Some(handle),
            ..self
        }
    }

    pub fn with_buffer(self, buffer: usize) -> Self {
        Self { buffer, ..self }
    }

    pub fn build(self) -> Result<Anotify>
    where
        B: crate::binding::Binding + Send + 'static,
        B::Identifier: std::fmt::Debug + Eq + Hash + Send,
    {
        let (shared, requests) = SharedState::with_capacity(self.buffer);

        let binding = B::new()?;

        let task_state = TaskState::new(shared.clone(), requests, binding)?;

        let handle = self
            .handle
            .unwrap_or_else(|| tokio::runtime::Handle::current());

        let jh = task_state.launch_in(handle);

        let inner = super::handle::AnotifyHandle { shared };

        Ok(Anotify {
            cancel_on_drop: true,
            inner,
            jh,
        })
    }
}
