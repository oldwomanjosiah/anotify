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
    Close,
}
