use std::{
    borrow::Cow,
    ffi::{CStr, CString},
    path::{Path, PathBuf},
    slice,
    sync::Arc,
};

mod ffi;
mod split;

#[cfg(test)]
mod test;

use bytes::{Buf, BufMut};

pub use bytes;
pub use split::{OwnedReadHalf, OwnedWriteHalf};

/// Builder for creating a new channel or opening an existing one
pub struct OpenOptions {
    flags: i32,
    perm: u32,
    bufsize: usize,
}

impl OpenOptions {
    /// Create a new channel builder.
    pub fn new() -> Self {
        Self {
            flags: 0,
            perm: 0o644, // Default permission
            bufsize: 0,  // Default buffer size
        }
    }

    /// Sets the option to create a new file, or open it if it already exists.
    pub fn create(self, create: bool, bufsize: usize) -> Self {
        Self {
            flags: self.flags | if create { ffi::KZ_CREATE } else { 0 },
            perm: self.perm,
            bufsize,
        }
    }

    /// Sets the permission for the channel file.
    pub fn perm(self, perm: u32) -> Self {
        Self {
            flags: self.flags,
            perm,
            bufsize: self.bufsize,
        }
    }

    /// Sets the option to create a new file, failing if it already exists.
    pub fn create_new(self, create: bool, bufsize: usize) -> Self {
        Self {
            flags: self.flags
                | if create {
                    ffi::KZ_CREATE | ffi::KZ_EXCL
                } else {
                    0
                },
            perm: self.perm,
            bufsize,
        }
    }

    /// Set the channel will be reset after opened.
    pub fn reset(self) -> Self {
        Self {
            flags: self.flags | ffi::KZ_RESET,
            perm: self.perm,
            bufsize: self.bufsize,
        }
    }

    /// Opens an channel with name and the options specified by self.
    pub fn open(self, name: impl AsRef<Path>) -> IoResult<Channel> {
        Channel::raw_open(name, self.flags | self.perm as i32, self.bufsize)
    }
}

pub struct Channel {
    ptr: *mut ffi::kz_State,
}

unsafe impl Send for Channel {}
unsafe impl Sync for Channel {}

impl Drop for Channel {
    fn drop(&mut self) {
        unsafe { ffi::kz_close(self.ptr) }
    }
}

impl std::fmt::Debug for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Channel(\"{}\", {:p})", self.name(), self.as_ptr())
    }
}

impl AsRef<Path> for Channel {
    fn as_ref(&self) -> &Path {
        unsafe { self.name_str().as_ref() }
    }
}

impl Channel {
    /// Maximum size in bytes of the queue in channel
    pub const MAX_SIZE: usize = ffi::KZ_MAX_SIZE;

    /// Calculate buffer size that is aligned to page size (with header)
    ///
    /// returns the buffer size that makes shared memory size requested aligned
    /// to page size. so the returned size could a little less than the aligned
    /// page size
    pub fn aligned(required_size: usize, page_size: usize) -> usize {
        unsafe { ffi::kz_aligned(required_size, page_size) }
    }

    /// Check if shm file exists
    pub fn exists(name: impl AsRef<Path>) -> IoResult<Option<(i32, i32)>> {
        let name =
            CString::new(name.as_ref().to_string_lossy().as_bytes()).unwrap();
        let (mut owner, mut user) = (0, 0);
        let r =
            unsafe { ffi::kz_exists(name.as_ptr(), &mut owner, &mut user) };
        if r < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if r == 0 {
            return Ok(None);
        }
        Ok(Some((owner, user)))
    }

    /// Unlink shm file
    pub fn unlink(name: impl AsRef<Path>) -> IoResult<()> {
        let name =
            CString::new(name.as_ref().to_string_lossy().as_bytes()).unwrap();
        let r = unsafe { ffi::kz_unlink(name.as_ptr()) };
        if r < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    /// Create new shm channel, bufsize is the total size of two queues in buffer
    pub fn create(name: impl AsRef<Path>, bufsize: usize) -> IoResult<Self> {
        Self::raw_open(name, ffi::KZ_CREATE, bufsize)
    }

    /// Same as `create`, but will fail if the channel already exists.
    pub fn create_new(
        name: impl AsRef<Path>,
        bufsize: usize,
    ) -> IoResult<Self> {
        Self::raw_open(name, ffi::KZ_CREATE | ffi::KZ_EXCL, bufsize)
    }

    /// Open existing shm file
    pub fn open(name: impl AsRef<Path>) -> IoResult<Self> {
        Self::raw_open(name, 0, 0)
    }

    fn raw_open(
        name: impl AsRef<Path>,
        flags: i32,
        bufsize: usize,
    ) -> IoResult<Self> {
        let name =
            CString::new(name.as_ref().to_string_lossy().as_bytes()).unwrap();
        let ptr = unsafe { ffi::kz_open(name.as_ptr(), flags, bufsize) };
        if ptr.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self { ptr })
    }

    pub(crate) fn as_ptr(&self) -> *const ffi::kz_State {
        self.ptr
    }

    /// Create a guard that will shutdown the channel when dropped
    pub fn shutdown_guard(&self, mode: Mode) -> ShutdownGuard {
        ShutdownGuard(self.ptr, mode)
    }

    /// Create a guard that will unlink the shm file when dropped
    pub fn unlink_guard(&self) -> UnlinkGuard {
        let name = unsafe { CStr::from_ptr(ffi::kz_name(self.ptr)) }
            .to_string_lossy()
            .to_string();
        UnlinkGuard::new(name)
    }

    /// Channel name
    pub fn name(&self) -> Cow<'_, str> {
        unsafe { CStr::from_ptr(ffi::kz_name(self.ptr)) }.to_string_lossy()
    }

    unsafe fn name_str(&self) -> &str {
        let name = unsafe { CStr::from_ptr(ffi::kz_name(self.ptr)) };
        name.to_str().unwrap()
    }

    /// Queue size in bytes of the channel
    pub fn size(&self) -> usize {
        unsafe { ffi::kz_size(self.ptr) }
    }

    /// Current process id
    pub fn pid(&self) -> i32 {
        unsafe { ffi::kz_pid(self.ptr) }
    }

    /// Check if the current process is the owner of the channel
    pub fn is_owner(&self) -> bool {
        unsafe { ffi::kz_isowner(self.ptr) != 0 }
    }

    /// Check if the channel is closed
    pub fn is_closed(&self, mode: Mode) -> bool {
        let mode = mode.as_raw();
        if mode == 0 {
            return unsafe { ffi::kz_isclosed(self.ptr) != 0 };
        }
        unsafe { (ffi::kz_isclosed(self.ptr) & mode) == mode }
    }

    /// Runs f, and ignore the `Closed` error
    pub fn with_closed_handled(
        &self,
        f: impl FnOnce(&Self) -> Result<()>,
    ) -> Result<()> {
        match f(self) {
            Ok(()) => Ok(()),
            Err(Error::Closed) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Split the channel into read and write parts
    pub fn into_split(self) -> (OwnedReadHalf, OwnedWriteHalf) {
        let rc = Arc::new(self);
        return (OwnedReadHalf::new(rc.clone()), OwnedWriteHalf::new(rc));
    }

    /// Shutdown the channel
    pub fn shutdown(&self, mode: Mode) -> Result<()> {
        let r = unsafe { ffi::kz_shutdown(self.ptr, mode.as_raw()) };
        Error::get_result(r, ())
    }

    /// Wait if the channel is not ready for read/write
    pub fn wait(&self, request_size: usize) -> Result<Mode> {
        self.wait_util(request_size, -1)
    }

    /// Wait if the channel is not ready for read/write with timeout
    pub fn wait_util(&self, request_size: usize, millis: i32) -> Result<Mode> {
        let r = unsafe { ffi::kz_wait(self.ptr, request_size, millis) };
        if r < 0 {
            return Err(Error::from_retcode(r));
        }
        Ok(Mode(r))
    }

    /// Read data from the channel
    pub fn read(&self, write: impl BufMut) -> Result<usize> {
        self.read_util(write, -1)
    }

    /// Read data from the channel with timeout
    pub fn read_util(
        &self,
        mut write: impl BufMut,
        millis: i32,
    ) -> Result<usize> {
        let mut ctx = self.read_context()?;
        ctx = ctx.wait_util(millis)?;
        Ok(ctx.read(&mut write)?)
    }

    /// Write data to the channel
    pub fn write(&self, data: impl Buf) -> Result<()> {
        self.write_util(data, -1)
    }

    /// Write data to the channel with timeout
    pub fn write_util(&self, data: impl Buf, millis: i32) -> Result<()> {
        let len = data.remaining();
        let mut ctx = self.write_context(len)?;
        ctx = ctx.wait_util(millis)?;
        ctx.write(data)?;
        Ok(())
    }

    /// create a context for read operation
    pub fn read_context(&self) -> Result<Context<'_>> {
        let mut ctx = std::mem::MaybeUninit::uninit();
        let r = unsafe { ffi::kz_read(self.ptr, ctx.as_mut_ptr()) };
        if r != ffi::KZ_OK && r != ffi::KZ_AGAIN {
            return Err(Error::from_retcode(r));
        }
        Ok(Context {
            raw: unsafe { ctx.assume_init() },
            _marker: std::marker::PhantomData,
        })
    }

    /// create a context for write operation
    pub fn write_context(&self, len: usize) -> Result<Context<'_>> {
        let mut ctx = std::mem::MaybeUninit::uninit();
        let r = unsafe { ffi::kz_write(self.ptr, ctx.as_mut_ptr(), len) };
        if r != ffi::KZ_OK && r != ffi::KZ_AGAIN {
            return Err(Error::from_retcode(r));
        }
        Ok(Context {
            raw: unsafe { ctx.assume_init() },
            _marker: std::marker::PhantomData,
        })
    }
}

/// Context used to perform read/write operations
pub struct Context<'a> {
    raw: ffi::kz_Context,
    _marker: std::marker::PhantomData<&'a ()>,
}

unsafe impl Send for Context<'_> {}

impl Context<'_> {
    /// read data from Context.
    pub fn read(self, mut write: impl BufMut) -> Result<usize> {
        if !self.is_read() || self.would_block() {
            return Err(Error::Invalid);
        }
        let data = self.buffer();
        let len = data.len();
        if write.remaining_mut() < len {
            return Err(Error::TooBig);
        }
        write.put_slice(data);
        self.commit(0)?;
        Ok(len)
    }

    /// write data to Context.
    pub fn write(mut self, mut data: impl Buf) -> Result<usize> {
        if self.is_read() || self.would_block() {
            return Err(Error::Invalid);
        }
        let buf = self.buffer_mut();
        let len = data.remaining();
        if buf.len() < len {
            return Err(Error::TooBig);
        }
        data.copy_to_slice(&mut buf[..len]);
        self.commit(len)?;
        Ok(len)
    }

    /// Check if the context is used for read operation.
    pub fn is_read(&self) -> bool {
        unsafe { ffi::kz_isread(&self.raw) != 0 }
    }

    /// Check if the context should call `wait` first.
    pub fn would_block(&self) -> bool {
        self.raw.result == ffi::KZ_AGAIN
    }

    /// Make a static context from this context, used by waiting on another
    /// thread.
    ///
    /// SAFETY: You must ensure that only one thread is using this context at a time.
    pub unsafe fn into_static(self) -> Context<'static> {
        Context {
            raw: self.raw,
            _marker: std::marker::PhantomData,
        }
    }

    /// Wait until the channel is ready for read/write.
    pub fn wait(self) -> Result<Self> {
        self.wait_util(-1)
    }

    /// Wait until the channel is ready for read/write with timeout.
    pub fn wait_util(mut self, millis: i32) -> Result<Self> {
        let r = unsafe { ffi::kz_waitcontext(&mut self.raw, millis) };
        Error::get_result(r, self)
    }

    /// Cancel the read/write operation of this context
    pub fn cancel(&mut self) {
        unsafe { ffi::kz_cancel(&mut self.raw) }
    }

    /// Returns a slice of the read buffer
    pub fn buffer(&self) -> &[u8] {
        unsafe {
            let mut len = 0;
            let p = ffi::kz_buffer(&self.raw as *const _ as *mut _, &mut len);
            if len == 0 {
                &[]
            } else {
                slice::from_raw_parts(p.cast(), len)
            }
        }
    }

    /// Returns a mutable reference to the write buffer.
    pub fn buffer_mut(&mut self) -> &mut [u8] {
        unsafe {
            let mut len = 0;
            let p = ffi::kz_buffer(&mut self.raw, &mut len);
            if len == 0 {
                &mut []
            } else {
                slice::from_raw_parts_mut(p.cast(), len)
            }
        }
    }

    /// Commit copied data with actual length
    pub fn commit(mut self, len: usize) -> Result<()> {
        let code = unsafe { ffi::kz_commit(&mut self.raw, len) };
        Error::get_result(code, ())
    }
}

/// Mode used to indicate the channel is ready for read/write
#[derive(Clone, Copy)]
pub struct Mode(i32);

impl std::fmt::Debug for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            ffi::KZ_READ => write!(f, "Mode(READ)"),
            ffi::KZ_WRITE => write!(f, "Mode(WRITE)"),
            ffi::KZ_BOTH => write!(f, "Mode(BOTH)"),
            code => write!(f, "Mode(0x{:x})", code),
        }
    }
}

impl Mode {
    /// NOT_READY is used to indicate that the channel is not ready for read/write
    pub const NULL: Self = Self(0);
    /// READ is used to indicate that the channel is ready for read
    pub const READ: Self = Self(ffi::KZ_READ);
    /// WRITE is used to indicate that the channel is ready for write
    pub const WRITE: Self = Self(ffi::KZ_WRITE);
    /// BOTH is used to indicate that the channel is ready for both read and write
    pub const BOTH: Self = Self(ffi::KZ_BOTH);

    /// attach `READ` tag to the mode
    pub fn with_read(self, is_read: bool) -> Self {
        Mode(self.0 | if is_read { ffi::KZ_READ } else { 0 })
    }

    /// attach `WRITE` tag to the mode
    pub fn with_write(self, is_write: bool) -> Self {
        Mode(self.0 | if is_write { ffi::KZ_WRITE } else { 0 })
    }

    /// Check if the mode indicates channel is ready for read
    pub fn can_read(&self) -> bool {
        self.0 & ffi::KZ_READ != 0
    }

    /// Check if the mode indicates channel is ready for write
    pub fn can_write(&self) -> bool {
        self.0 & ffi::KZ_WRITE != 0
    }

    /// Check if the mode indicates channel is ready for whether read or write
    pub fn is_ready(&self) -> bool {
        self.0 != 0
    }

    fn as_raw(&self) -> i32 {
        self.0
    }
}

/// A shutdown guard, will close the channel when dropped
#[derive(Debug)]
pub struct ShutdownGuard(*mut ffi::kz_State, Mode);

unsafe impl Send for ShutdownGuard {}

impl Drop for ShutdownGuard {
    fn drop(&mut self) {
        unsafe { ffi::kz_shutdown(self.0, self.1.as_raw()) };
    }
}

pub type Result<T> = std::result::Result<T, Error>;

type IoResult<T> = std::result::Result<T, std::io::Error>;

/// A guard that will unlink the channel when dropped
#[derive(Debug)]
pub struct UnlinkGuard {
    name: PathBuf,
}

impl Drop for UnlinkGuard {
    fn drop(&mut self) {
        if let Err(err) = Channel::unlink(&self.name) {
            if err.kind() == std::io::ErrorKind::NotFound {
                // ignore not found error
                return;
            }
            panic!(
                "Failed to unlink channel {}: {}",
                self.name.display(),
                err
            );
        }
    }
}

impl UnlinkGuard {
    /// Create a new unlink guard
    pub fn new(name: impl AsRef<Path>) -> Self {
        let name = name.as_ref().to_path_buf();
        Self { name }
    }
}

/// Error type for the library
#[derive(Debug)]
pub enum Error {
    Ok,
    Invalid,
    Fail(std::io::Error),
    Closed,
    TooBig,
    Again,
    Busy,
    Timeout,
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Fail(err) => Some(err),
            _ => None,
        }
    }
}

impl Error {
    /// Returns the system error if the error is a failure
    pub fn fail_error(&self) -> Option<&std::io::Error> {
        match self {
            Error::Fail(err) => Some(err),
            _ => None,
        }
    }

    fn get_result<T>(code: i32, ok: T) -> Result<T> {
        match code {
            ffi::KZ_OK => Ok(ok),
            _ => Err(Error::from_retcode(code)),
        }
    }

    fn from_retcode(code: i32) -> Self {
        match code {
            ffi::KZ_OK => Error::Ok,
            ffi::KZ_INVALID => Error::Invalid,
            ffi::KZ_FAIL => Self::last_os_error(),
            ffi::KZ_CLOSED => Error::Closed,
            ffi::KZ_TOOBIG => Error::TooBig,
            ffi::KZ_AGAIN => Error::Again,
            ffi::KZ_BUSY => Error::Busy,
            ffi::KZ_TIMEOUT => Error::Timeout,
            _ => Error::Fail(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Unknown error({})", code),
            )),
        }
    }

    fn last_os_error() -> Self {
        Error::Fail(std::io::Error::last_os_error())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Ok => write!(f, "No error"),
            Error::Invalid => write!(f, "Invalid operation"),
            Error::Fail(msg) => write!(f, "OS Error: {}", msg),
            Error::Closed => write!(f, "Channel is closed"),
            Error::TooBig => write!(f, "Data is too big"),
            Error::Again => write!(f, "Try again"),
            Error::Busy => write!(f, "Resource is busy"),
            Error::Timeout => write!(f, "Operation timed out"),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Fail(err)
    }
}

impl Into<Option<std::io::Error>> for Error {
    fn into(self) -> Option<std::io::Error> {
        match self {
            Error::Fail(err) => Some(err),
            _ => None,
        }
    }
}

impl Into<std::io::Error> for Error {
    fn into(self) -> std::io::Error {
        use std::io::Error as IoError;
        use std::io::ErrorKind;
        match self {
            Error::Ok => IoError::new(ErrorKind::Other, self),
            Error::Invalid => IoError::new(ErrorKind::InvalidInput, self),
            Error::Fail(msg) => IoError::new(ErrorKind::Other, msg),
            Error::Closed => IoError::new(ErrorKind::BrokenPipe, self),
            Error::TooBig => IoError::new(ErrorKind::OutOfMemory, self),
            Error::Again => IoError::new(ErrorKind::WouldBlock, self),
            Error::Busy => IoError::new(ErrorKind::ResourceBusy, self),
            Error::Timeout => IoError::new(ErrorKind::TimedOut, self),
        }
    }
}
