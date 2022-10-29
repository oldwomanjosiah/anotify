use std::{
    hash::Hash,
    path::{Path, PathBuf},
};

use crate::new::external::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BindingEventType {
    Open,
    CloseNoModify,
    CloseModify,
    Read,
    Write,
    Metadata,
    Create,
    Delete,
    MoveFrom { cookie: u32 },
    MoveTo { cookie: u32 },
    SelfRemoved,
}

impl BindingEventType {
    /// Watch is closing from this, consumers should be notified.
    pub fn closing(&self) -> bool {
        matches!(self, BindingEventType::SelfRemoved)
    }
}

#[derive(Debug)]
pub struct BindingEvent<I> {
    pub wd: I,
    pub path: Option<PathBuf>,
    pub ty: Vec<BindingEventType>,
}

impl<I> BindingEvent<I> {
    /// Watch is closing from this, consumers should be notified and watch removed from binding.
    pub fn closing(&self) -> bool {
        self.ty.iter().any(BindingEventType::closing)
    }
}

pub trait Binding {
    type Identifier: PartialEq + PartialOrd + Hash + Copy + 'static;

    /// Create a new watch
    fn create(
        &mut self,
        path: impl AsRef<Path>,
        flags: crate::new::external::EventFilter,
    ) -> Result<Self::Identifier>;

    /// Update an existing watch
    fn update(
        &mut self,
        id: Self::Identifier,
        path: impl AsRef<Path>,
        flags: crate::new::external::EventFilter,
    ) -> Result<Self::Identifier>;

    /// Remove an existing watch
    fn remove(&mut self, id: Self::Identifier) -> Result<()>;

    /// Poll for the next set of events from a binding.
    fn poll_events(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<Vec<BindingEvent<Self::Identifier>>>>
    where
        Self: Sized;

    fn events(&mut self) -> Next<'_, Self>
    where
        Self: Sized,
    {
        Next(self)
    }
}

/// Future type for the next set of events from a binding.
pub struct Next<'b, B>(&'b mut B);

impl<'b, B: Binding> std::future::Future for Next<'b, B> {
    type Output = std::io::Result<Vec<BindingEvent<B::Identifier>>>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        (&mut self.0).poll_events(cx)
    }
}
