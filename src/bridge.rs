use crate::errors::Result;
use crate::events::{Event, EventFilter};
use crate::shared::Id;

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
