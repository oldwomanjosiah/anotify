use displaydoc::Display;
use thiserror::Error;

/// Top level error that can be used to collect more specific errors yielded by library components
#[derive(Debug, Error, Display)]
pub enum AnotifyError {
    /// Failure to initialize the Anotify Watch Handler
    Init(InitError),
}

/// Failure to initialize the Anotify Watch Handler
#[derive(Debug, Error, Display)]
pub enum InitError {
    /// Failed to initialize inotify instance with operating system, got errno {0}
    Inotify(#[from] nix::errno::Errno),

    /// Failed to register inotify instance instance with tokio io driver
    AsyncFd(#[from] std::io::Error),
}

macro_rules! intoerror {
    () => {};

    ($from:ty => $discriminant:ident; $($rest:tt)*) => {
        impl From<$from> for AnotifyError {
            fn from(_: $from) -> Self {
                Self::$discriminant
            }
        }

        intoerror!($($rest)*);
    };

    ($from:ty => $discriminant:ident ($using:ident); $($rest:tt)*) => {
        impl From<$from> for AnotifyError {
            fn from($using: $from) -> Self {
                Self::$discriminant($using)
            }
        }

        intoerror!($($rest)*);
    }
}

intoerror! {
    InitError => Init(it);
}
