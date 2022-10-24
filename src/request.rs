use std::path::PathBuf;

use crate::{
    handle::{WatchError, WatchType},
    task::WatchRequestInner,
};

/// Full Configuration For a New Watch
#[non_exhaustive]
pub struct RequestConfig<T: RequestType> {
    pub path: T,
    pub flags: T::Flags,
}

impl<T: RequestType> std::default::Default for RequestConfig<T>
where
    T: Default,
    T::Flags: Default,
{
    fn default() -> Self {
        Self {
            path: Default::default(),
            flags: Default::default(),
        }
    }
}

pub trait IntoRequest {
    type Stream;
    type Once;

    // TODO(josiah) Update to use a opaque public type,
    // which can be unsafely built from it's components.

    fn into_stream(self) -> (Self::Stream, WatchRequestInner);
    fn into_once(self) -> (Self::Once, WatchRequestInner);
}

pub trait RequestType {
    type Flags;
}

#[derive(Debug, Default)]
pub struct File(pub PathBuf);

#[derive(Debug, Default)]
pub struct Directory(pub PathBuf);

impl File {
    fn new<T>(inner: T) -> Self
    where
        T: Into<PathBuf>,
    {
        Self(inner.into())
    }
}

impl Directory {
    fn new<T>(inner: T) -> Self
    where
        T: Into<PathBuf>,
    {
        Self(inner.into())
    }
}

#[enumflags2::bitflags(default = Modify)]
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFlags {
    Read,
    Modify,
    Close,
    Open,
}

impl RequestType for File {
    type Flags = enumflags2::BitFlags<FileFlags>;
}

impl RequestType for Directory {
    type Flags = enumflags2::BitFlags<FileFlags>;
}

impl IntoRequest for RequestConfig<File> {
    type Stream = crate::futures::FileWatchStream;
    type Once = crate::futures::FileWatchFuture;

    fn into_once(self) -> (Self::Once, WatchRequestInner) {
        todo!()
    }

    fn into_stream(self) -> (Self::Stream, WatchRequestInner) {
        todo!()
    }
}

impl IntoRequest for RequestConfig<Directory> {
    type Stream = crate::futures::DirectoryWatchStream;
    type Once = crate::futures::DirectoryWatchFuture;

    fn into_once(self) -> (Self::Once, WatchRequestInner) {
        todo!()
    }

    fn into_stream(self) -> (Self::Stream, WatchRequestInner) {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    #[tokio::test]
    async fn test() {
        let mut handle = new().unwrap();

        handle
            .stream(RequestConfig {
                path: File::new("./test.md"),
                flags: FileFlags::Modify | FileFlags::Close,
                ..RequestConfig::default()
            })
            .unwrap()
            .await;

        handle
            .stream(RequestConfig {
                path: Directory::new("./src/"),
                flags: FileFlags::Open | FileFlags::Read,
                ..RequestConfig::default()
            })
            .unwrap()
            .await;

        drop(handle);
    }
}
