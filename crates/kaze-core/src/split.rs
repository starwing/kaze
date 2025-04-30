use std::sync::Arc;

use bytes::{Buf, BufMut};

use crate::{Channel, Context, Mode, ShutdownGuard};

/// A write part of the channel
pub struct OwnedWriteHalf {
    pub(crate) channel: Arc<Channel>,
    shutdown_on_drop: bool,
}

impl Drop for OwnedWriteHalf {
    fn drop(&mut self) {
        if self.shutdown_on_drop {
            let _ = self.channel.shutdown(Mode::WRITE);
        }
    }
}

impl std::fmt::Debug for OwnedWriteHalf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "OwnedWriteHalf(\"{}\", {:p})",
            self.channel.name(),
            self.channel.as_ptr()
        )
    }
}

impl OwnedWriteHalf {
    pub(crate) fn new(channel: Arc<Channel>) -> Self {
        OwnedWriteHalf {
            channel,
            shutdown_on_drop: true,
        }
    }

    /// Forget the shutdown on drop
    pub fn forget(mut self) {
        self.shutdown_on_drop = false;
        drop(self);
    }

    /// Reunite with another read part to create a channel
    pub fn reunite(
        self,
        other: OwnedReadHalf,
    ) -> std::result::Result<Channel, ReuniteError> {
        reunite(self, other)
    }

    /// Create a guard for shutdown the read part when dropped
    pub fn shutdown_lock(&self) -> ShutdownGuard {
        self.channel.shutdown_guard(Mode::READ)
    }

    /// Write data to the channel
    pub fn write(&self, data: impl Buf) -> crate::Result<()> {
        self.channel.write_util(data, -1)
    }

    /// Write data to the channel with timeout
    pub fn write_util(
        &self,
        data: impl Buf,
        millis: i32,
    ) -> crate::Result<()> {
        self.channel.write_util(data, millis)
    }

    /// create a context for write operation
    pub fn write_context(&self, len: usize) -> crate::Result<Context<'_>> {
        self.channel.write_context(len)
    }
}

/// A read part of the channel
#[derive(Clone)]
pub struct OwnedReadHalf {
    pub(crate) channel: Arc<Channel>,
}

impl std::fmt::Debug for OwnedReadHalf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "OwnedReadHalf(\"{}\", {:p})",
            self.channel.name(),
            self.channel.as_ptr()
        )
    }
}

impl OwnedReadHalf {
    pub(crate) fn new(channel: Arc<Channel>) -> Self {
        OwnedReadHalf { channel }
    }

    /// Reunite with another write part to create a channel
    pub fn reunite(
        self,
        other: OwnedWriteHalf,
    ) -> std::result::Result<Channel, ReuniteError> {
        reunite(other, self)
    }

    /// Create a guard for shutdown the read part when dropped
    pub fn shutdown_lock(&self) -> ShutdownGuard {
        self.channel.shutdown_guard(Mode::READ)
    }

    /// Shutdown the read part
    pub fn shutdown(&self) -> crate::Result<()> {
        self.channel.shutdown(Mode::BOTH)
    }

    /// Read data from the channel
    pub fn read(&self, write: impl BufMut) -> crate::Result<usize> {
        self.channel.read_util(write, -1)
    }

    /// Read data from the channel with timeout
    pub fn read_util(
        &self,
        write: impl BufMut,
        millis: i32,
    ) -> crate::Result<usize> {
        self.channel.read_util(write, millis)
    }

    /// create a context for read operation
    pub fn read_context(&self) -> crate::Result<Context<'_>> {
        self.channel.read_context()
    }
}

/// Error indicating that two halves were not from the same socket, and thus could
/// not be reunited.
#[derive(Debug)]
pub struct ReuniteError(pub OwnedReadHalf, pub OwnedWriteHalf);

fn reunite(
    write: OwnedWriteHalf,
    read: OwnedReadHalf,
) -> std::result::Result<Channel, ReuniteError> {
    if Arc::ptr_eq(&read.channel, &write.channel) {
        write.forget();
        // This unwrap cannot fail as the api does not allow creating more than two Arcs,
        // and we just dropped the other half.
        Ok(Arc::try_unwrap(read.channel)
            .expect("Channel: try_unwrap failed in reunite"))
    } else {
        Err(ReuniteError(read, write))
    }
}
