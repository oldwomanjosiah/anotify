use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    path::PathBuf,
};

use crate::new::{EventFilter, EventType};

use super::{
    super::Event,
    binding::{Binding, BindingEvent, BindingEventType},
    Id,
};

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
    move_cache: HashMap<u32, PathBuf>,
    // TODO(josiah) consider adding map from path to I for new inserts
}

impl<I> Registry<I> {
    pub fn new() -> Self {
        Self {
            collectors: HashMap::new(),
            watches: HashMap::new(),
            move_cache: HashMap::new(),
        }
    }

    fn collector(&self, id: Id) -> Option<&Collector<I>> {
        self.collectors.get(&id)
    }

    fn cache_or_take(&mut self, cookie: u32, from: bool, path: PathBuf) -> Option<Event> {
        if let Some(other) = self.move_cache.remove(&cookie) {
            if from {
                Some(Event {
                    path: path,
                    ty: EventType::Move { to: Some(other) },
                })
            } else {
                Some(Event {
                    path: other,
                    ty: EventType::Move { to: Some(path) },
                })
            }
        } else {
            self.move_cache.insert(cookie, path);
            None
        }
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

    /// Convert binding events into user events.
    fn convert_event(&mut self, wd: I, event: BindingEvent<I>) -> Vec<Event> {
        let BindingEvent {
            wd: _,
            path,
            ty: tys,
        } = event;

        let Some(path) = path.or_else(|| {
            self.watches.get(&wd).map(|it| it.path.clone())
        }) else {
            return Vec::new();
        };

        tys.into_iter()
            .filter_map(|ty| {
                let path = path.clone();

                let ty = match ty {
                    BindingEventType::Open => EventType::Open,
                    BindingEventType::CloseNoModify => EventType::Close { modified: false },
                    BindingEventType::CloseModify => EventType::Close { modified: true },
                    BindingEventType::Read => EventType::Read,
                    BindingEventType::Write => EventType::Write,
                    BindingEventType::Metadata => EventType::Metadata,
                    BindingEventType::Create => EventType::Create,
                    BindingEventType::Delete => EventType::Delete,
                    BindingEventType::SelfRemoved => EventType::Delete,

                    BindingEventType::MoveFrom { cookie } => {
                        return self.cache_or_take(cookie, true, path);
                    }
                    BindingEventType::MoveTo { cookie } => {
                        return self.cache_or_take(cookie, false, path);
                    }
                };

                Some(Event { path, ty })
            })
            .collect()
    }

    /// Try and send a set of events to their listening collectors.
    ///
    /// Returns a list of collectors who should be removed after these events have been sent.
    pub fn try_send_events(
        &mut self,
        events: Vec<BindingEvent<I>>,
    ) -> crate::new::error::Result<HashSet<Id>> {
        let mut to_remove = HashSet::new();

        for event in events.into_iter() {
            let wd = event.wd;
            let events = self.convert_event(event.wd, event);

            let Some(watch) = self.watches.get(&wd) else {
                tracing_impl::info!(?wd, "Watch was removed for id before event could be processed");
                continue;
            };

            'collectors: for id in watch.interested.iter() {
                let Some(collector) = self.collectors.get(id) else {
                    unreachable!("Collector with {id:?} was removed without removing it's interest from a watch {wd:?}");
                };

                for event in events.iter() {
                    if event.contained_in(&collector.filter) {
                        use tokio::sync::mpsc::error::TrySendError;

                        match collector.sender.try_send(event.clone()) {
                            Ok(()) => {}
                            Err(TrySendError::Full(_)) => {
                                tracing_impl::info!(?id, "Could not send event, sender full");
                                continue 'collectors;
                            }
                            Err(TrySendError::Closed(_)) => {
                                tracing_impl::trace!(?id, "Removing collector, sender closed");
                                to_remove.insert(*id);
                                continue 'collectors;
                            }
                        }

                        if collector.once {
                            tracing_impl::trace!(
                                ?id,
                                "Removing collector, requested only single event"
                            );
                            to_remove.insert(*id);
                            continue 'collectors;
                        }
                    }
                }
            }
        }

        Ok(to_remove)
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
