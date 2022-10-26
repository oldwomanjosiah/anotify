use std::{
    hash::Hash,
    path::{Path, PathBuf},
};

use crate::new::external::Result;

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
    fn should_remove_watch(&self) -> bool {
        matches!(self, BindingEventType::SelfRemoved)
    }
}

pub struct BindingEvent<B: Binding> {
    pub wd: B::Identifier,
    pub path: Option<PathBuf>,
    pub ty: Vec<BindingEventType>,
}

pub trait Binding: Sized {
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

    fn poll_events(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<Vec<BindingEvent<Self>>>>;
}
