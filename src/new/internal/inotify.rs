use std::{
    hash::Hash,
    ops::Deref,
    os::unix::prelude::{AsRawFd, FromRawFd, OwnedFd},
    path::{Path, PathBuf},
};

use nix::sys::inotify::Inotify;
use tokio::io::{unix::AsyncFd, Interest};

use crate::new::{
    external::{AnotifyError, AnotifyErrorType, Result},
    EventFilter,
};

use super::binding::{Binding, BindingEvent, BindingEventType};

mod stats;

struct OwnedInotify(Inotify);

impl Drop for OwnedInotify {
    fn drop(&mut self) {
        // SAFETY:
        // - Drop is guaranteed to be called at most once
        // - Drop guarantees that we have exclusive access to self
        // - No references to self may exist after drop
        // - from_raw_fd requires that it is safe to forge ownership
        drop(unsafe { OwnedFd::from_raw_fd(self.0.as_raw_fd()) });
    }
}

impl Deref for OwnedInotify {
    type Target = Inotify;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRawFd for OwnedInotify {
    fn as_raw_fd(&self) -> std::os::unix::prelude::RawFd {
        self.0.as_raw_fd()
    }
}

/// Platform bindings for [`Inotify`][`nix::sys::inotify::Inotify`]
pub struct InotifyBinding {
    fd: AsyncFd<OwnedInotify>,
    stats: stats::Stats,
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct WatchIdentifier(nix::sys::inotify::WatchDescriptor);

impl InotifyBinding {
    /// Create a new platform binding for inotify
    pub fn new() -> Result<Self> {
        use nix::sys::inotify::*;
        let inotify = Inotify::init(InitFlags::IN_NONBLOCK | InitFlags::IN_CLOEXEC)
            .map_err(Self::convert_error)?;

        let fd = AsyncFd::with_interest(OwnedInotify(inotify), Interest::READABLE).unwrap();

        let stats = stats::Stats::new();

        Ok(Self { fd, stats })
    }

    fn create_mask(flags: EventFilter, update: bool) -> nix::sys::inotify::AddWatchFlags {
        use crate::new::EventFilterType;
        use nix::sys::inotify::AddWatchFlags;

        let mut out = AddWatchFlags::IN_DELETE_SELF | AddWatchFlags::IN_MOVE_SELF;

        for flag in flags.into_iter() {
            out |= match flag {
                EventFilterType::Read => AddWatchFlags::IN_ACCESS,
                EventFilterType::Write => AddWatchFlags::IN_MODIFY,
                EventFilterType::Open => AddWatchFlags::IN_OPEN,
                EventFilterType::CloseNoModify => AddWatchFlags::IN_CLOSE_NOWRITE,
                EventFilterType::CloseModify => AddWatchFlags::IN_CLOSE_WRITE,
                EventFilterType::Move => AddWatchFlags::IN_MOVE,
                EventFilterType::Metadata => AddWatchFlags::IN_ATTRIB,
                EventFilterType::Create => AddWatchFlags::IN_CREATE,
                EventFilterType::Delete => AddWatchFlags::IN_DELETE,
                EventFilterType::DirOnly | EventFilterType::FileOnly => {
                    continue;
                }
            };
        }

        out
    }

    fn convert_mask(mask: nix::sys::inotify::AddWatchFlags, cookie: u32) -> Vec<BindingEventType> {
        macro_rules! check_mask {
            (fill $out:ident from { $($flags:expr => $body:expr,)* }) => {
                $(if mask.bits() & ($flags).bits() > 0 {
                    ($out).push($body);
                })*

                let taken = $($flags |)* ::nix::sys::inotify::AddWatchFlags::empty();
                let remainder = mask & !taken;

                if !remainder.is_empty() {
                    tracing_impl::warn!(?remainder, "Some Event Bits were not consumed");
                }
            };
        }

        use nix::sys::inotify::AddWatchFlags;
        let mut out = Vec::new();

        check_mask! { fill out from {
            AddWatchFlags::IN_OPEN => BindingEventType::Open,
            AddWatchFlags::IN_CLOSE_WRITE => BindingEventType::CloseModify,
            AddWatchFlags::IN_CLOSE_NOWRITE => BindingEventType::CloseNoModify,

            AddWatchFlags::IN_ACCESS => BindingEventType::Read,
            AddWatchFlags::IN_MODIFY => BindingEventType::Write,
            AddWatchFlags::IN_ATTRIB => BindingEventType::Metadata,

            AddWatchFlags::IN_CREATE => BindingEventType::Create,
            AddWatchFlags::IN_DELETE => BindingEventType::Delete,
            AddWatchFlags::IN_MOVED_TO => BindingEventType::MoveTo { cookie },
            AddWatchFlags::IN_MOVED_FROM => BindingEventType::MoveFrom { cookie },

            // TODO(josiah) this might not be what we want, but functionally it's the same
            // since the watch is being removed.
            AddWatchFlags::IN_DELETE_SELF
                | AddWatchFlags::IN_MOVE_SELF
                | AddWatchFlags::IN_UNMOUNT => BindingEventType::SelfRemoved,
        }};

        out
    }

    fn convert_error(error: nix::Error) -> AnotifyError {
        use nix::Error;

        let ty = match error {
            Error::EINVAL => {
                // This should always be considered a bug in the implementation,
                // crash so that we can find it faster.
                panic!(
                    "An invalid value was passed to some INotify flags! {}",
                    error
                );
            }
            Error::EMFILE | Error::ENFILE | Error::ENOMEM | Error::ENOSPC => {
                AnotifyErrorType::SystemResourceLimit
            }
            Error::EACCES => AnotifyErrorType::NoPermission,
            Error::ENAMETOOLONG => AnotifyErrorType::InvalidFilePath,
            Error::ENOENT => AnotifyErrorType::DoesNotExist,
            _ => AnotifyErrorType::Unknown {
                source: Box::new(error),
            },
        };

        AnotifyError::new(ty).with_message("Could not perform iNotify action")
    }

    fn convert_event(event: nix::sys::inotify::InotifyEvent) -> BindingEvent<Self> {
        BindingEvent {
            wd: WatchIdentifier(event.wd),
            path: event.name.map(|it| PathBuf::from(it)),
            ty: Self::convert_mask(event.mask, event.cookie),
        }
    }
}

impl Binding for InotifyBinding {
    type Identifier = WatchIdentifier;

    fn create(
        &mut self,
        path: impl AsRef<Path>,
        flags: crate::new::external::EventFilter,
    ) -> Result<Self::Identifier> {
        let wd = self
            .fd
            .get_mut()
            .add_watch(path.as_ref(), Self::create_mask(flags, false))
            .map_err(Self::convert_error)?;

        self.stats.inc_files(1);

        Ok(WatchIdentifier(wd))
    }

    fn update(
        &mut self,
        id: Self::Identifier,
        path: impl AsRef<Path>,
        flags: crate::new::external::EventFilter,
    ) -> Result<Self::Identifier> {
        let wd = self
            .fd
            .get_mut()
            .add_watch(path.as_ref(), Self::create_mask(flags, false))
            .map_err(Self::convert_error)?;

        assert_eq!(
            id.0, wd,
            "Update called with path that does not match existing watch!"
        );

        Ok(WatchIdentifier(wd))
    }

    fn remove(&mut self, id: Self::Identifier) -> Result<()> {
        let res = self
            .fd
            .get_mut()
            .rm_watch(id.0)
            .map_err(Self::convert_error);

        if res.is_ok() {
            self.stats.dec_files(1);
        }

        res
    }

    fn poll_events(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<Vec<BindingEvent<Self>>>> {
        use nix::Error;
        use std::task::{ready, Poll};

        let mut read_guard = match ready!(self.fd.poll_read_ready_mut(cx)) {
            Ok(it) => it,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    return std::task::Poll::Pending;
                }

                return std::task::Poll::Ready(Err(e));
            }
        };

        let mut events = Vec::new();

        loop {
            let new = match read_guard.get_inner().read_events() {
                Ok(events) => events,
                Err(e) => {
                    if e == Error::EWOULDBLOCK {
                        read_guard.clear_ready();

                        self.stats.note_events(events.len());

                        return Poll::Ready(Ok(events));
                    }

                    return Poll::Ready(Err(e.into()));
                }
            };

            events.reserve(new.len());
            for event in new.into_iter() {
                events.push(Self::convert_event(event));
            }
        }
    }
}
