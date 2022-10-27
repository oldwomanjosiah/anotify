use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    path::PathBuf,
};

use crate::new::EventFilter;

use super::{binding::Binding, Id};

mod bridge {
    use crate::new::{external::Event, internal::Id, EventFilter};
    use std::path::PathBuf;
    use tokio::sync::mpsc::{Receiver, Sender};

    pub type CollectorTx = Sender<Event>;
    pub type CollectorRx = Receiver<Event>;

    pub struct CollectorRequest {
        pub id: Id,
        pub path: PathBuf,
        pub once: bool,
        pub sender: CollectorTx,
        pub filter: EventFilter,
    }
}

use bridge::*;

/// Represents a single collector (single event, or stream), which has registered interest in some
/// file or directory.
#[derive(Debug)]
struct Collector<I> {
    wd: I,
    once: bool,
    sender: CollectorTx,
    filter: EventFilter,
}

/// Represents a single file or directory watch which has been installed into some
/// [`Binding`][`super::binding::Binding`].
#[derive(Debug)]
struct Watch {
    interested: HashSet<Id>,
    path: PathBuf,
    event_filter: EventFilter,
}

#[derive(Debug)]
pub struct Registry<I> {
    collectors: HashMap<Id, Collector<I>>,
    watches: HashMap<I, Watch>,
    // TODO(josiah) consider adding map from path to I for new inserts
}

impl<I> Registry<I> {
    pub fn new() -> Self {
        Self {
            collectors: HashMap::new(),
            watches: HashMap::new(),
        }
    }

    fn collector(&self, id: Id) -> Option<&Collector<I>> {
        self.collectors.get(&id)
    }
}

impl<I: Eq + Hash + Copy + std::fmt::Debug> Registry<I> {
    fn watches(&self, id: I) -> Option<&Watch> {
        self.watches.get(&id)
    }

    /// Register the interest of a new collector.
    pub fn register_interest<B>(
        &mut self,
        binding: &mut B,
        req: CollectorRequest,
    ) -> crate::new::error::Result<()>
    where
        B: Binding<Identifier = I>,
    {
        let CollectorRequest {
            id,
            path,
            once,
            sender,
            filter,
        } = req;

        if let Some((wd, it)) = self.watches.iter_mut().find(|(_, v)| v.path == path) {
            if let Some(new_filter) = it.register(id, filter) {
                binding.update(*wd, &it.path, new_filter).map(|_| ())
            } else {
                Ok(())
            }
        } else {
            let wd = binding.create(&path, filter)?;

            let new_watch = Watch {
                interested: HashSet::from([id]),
                path,
                event_filter: req.filter,
            };

            let new_collector = Collector {
                wd,
                once,
                sender,
                filter,
            };

            self.watches.insert(wd, new_watch);
            self.collectors.insert(req.id, new_collector);

            Ok(())
        }
    }

    /// Remove a collector from the registry.
    pub fn deregister_interest<B>(
        &mut self,
        binding: &mut B,
        id: Id,
    ) -> crate::new::error::Result<()>
    where
        B: Binding<Identifier = I>,
    {
        let Some(removing) = self.collectors.remove(&id) else {
            return Ok(());
        };

        let Some(watch) = self.watches.get_mut(&removing.wd) else {
            unreachable!("Watch was removed before last collector with a reference to it\nremoving: {removing:#?}");
        };

        if let Some(new_filter) =
            watch.deregister(id, |id| self.collectors.get(&id).map(|it| it.filter))
        {
            binding.update(removing.wd, &watch.path, new_filter)?;
        }

        if !watch.has_interest() {
            self.watches.remove(&removing.wd);
            binding.remove(removing.wd)?;
        }

        Ok(())
    }

    pub fn try_send_events<B>(
        &self,
        events: Vec<super::binding::BindingEvent<B>>,
    ) -> crate::new::error::Result<()>
    where
        B: Binding<Identifier = I>,
    {
        for event in events.into_iter() {
            let Some(watch) = self.watches.get(&event.wd) else {
                tracing_impl::info!(id = ?event.wd, "Watch was removed for id before event could be processed");
                continue;
            };

            todo!()
        }

        Ok(())
    }
}

impl Watch {
    /// Register the interest of a new watcher.
    fn register(&mut self, id: Id, filter: EventFilter) -> Option<EventFilter> {
        debug_assert!(
            !self.interested.insert(id),
            "Interest was added to the same watch twice"
        );

        if self.event_filter.contains(filter) {
            return None;
        }

        self.event_filter |= filter;

        Some(self.event_filter)
    }

    fn deregister(
        &mut self,
        id: Id,
        get: impl Fn(Id) -> Option<EventFilter>,
    ) -> Option<EventFilter> {
        if !self.interested.remove(&id) {
            return None;
        };

        if !self.has_interest() {
            return None;
        }

        let mut new = EventFilter::empty();

        for (id, filter) in self.interested.iter().map(|it| (*it, get(*it))) {
            if let Some(filter) = filter {
                new |= filter;
            } else {
                unreachable!(
                    "Collector was removed from the pool without being deregistered from the watch\n{id:?}"
                );
            }
        }

        let old = self.event_filter;
        self.event_filter = new;

        if new != old {
            Some(new)
        } else {
            None
        }
    }

    fn has_interest(&self) -> bool {
        !self.interested.is_empty()
    }
}
