use std::hash::Hash;

use crate::{errors::Result, handle::Anotify, shared::SharedState, task::TaskState};

pub struct AnotifyBuilder<B> {
    buffer: usize,
    handle: Option<tokio::runtime::Handle>,
    _phantom: std::marker::PhantomData<B>,
}

impl<B> AnotifyBuilder<B> {
    pub(crate) fn new() -> AnotifyBuilder<B> {
        AnotifyBuilder {
            buffer: SharedState::DEFAULT_CAPACITY,
            handle: None,
            _phantom: Default::default(),
        }
    }

    /// Set the runtime which will be used to collect the events.
    pub fn with_runtime(self, handle: tokio::runtime::Handle) -> Self {
        Self {
            handle: Some(handle),
            ..self
        }
    }

    /// Set the size of the request and event buffers
    ///
    /// Internal channels use bounded buffers, this sets the size for both the request buffer
    /// (maximum unhandled watch requests) and the per-watch event buffers (maximum unconsumed
    /// events for each stream).
    pub fn with_buffer(self, buffer: usize) -> Self {
        Self { buffer, ..self }
    }

    /// Set the platform [`Binding`][`crate::binding::Binding`]
    ///
    /// Often used with the Test Platform Binding to avoid
    /// side-effects in tests.
    pub fn with_platform<New>(self) -> AnotifyBuilder<New> {
        AnotifyBuilder {
            _phantom: Default::default(),
            buffer: self.buffer,
            handle: self.handle,
        }
    }

    /// Build the configured `Anotify` instance.
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
