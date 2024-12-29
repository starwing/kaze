use std::{
    borrow::Cow,
    ffi::{CStr, CString},
    io::{Error, ErrorKind, Result},
    mem::MaybeUninit,
    path::Path,
    slice,
};

mod bytes;
mod ffi;

pub use crate::bytes::{Bytes, BytesMut};

pub struct KazeState {
    ptr: *mut ffi::kz_State,
}

unsafe impl Send for KazeState {}
unsafe impl Sync for KazeState {}

impl Drop for KazeState {
    fn drop(&mut self) {
        unsafe { ffi::kz_delete(self.ptr) }
    }
}

impl KazeState {
    /// Check if shm queue exists
    pub fn exists(name: impl AsRef<Path>) -> Result<bool> {
        let name =
            CString::new(name.as_ref().to_string_lossy().as_bytes()).unwrap();
        match unsafe { ffi::kz_exists(name.as_ptr()) } {
            1 => Ok(true),
            0 => Ok(false),
            code => Err(Error::from_raw_os_error(code as i32)),
        }
    }

    /// Unlink shm queue
    pub fn unlink(name: impl AsRef<Path>) -> Result<()> {
        let name = CString::new(name.as_ref().to_string_lossy().as_bytes())?;
        match unsafe { ffi::kz_unlink(name.as_ptr()) } {
            ffi::KZ_OK => Ok(()),
            code => Err(Error::from_raw_os_error(code as i32)),
        }
    }

    /// Create new shm queue
    pub fn new(
        name: impl AsRef<Path>,
        ident: u32,
        bufsize: usize,
    ) -> Result<Self> {
        let name = CString::new(name.as_ref().to_string_lossy().as_bytes())?;
        let ptr = unsafe { ffi::kz_new(name.as_ptr(), ident, bufsize) };
        if ptr.is_null() {
            return Err(Error::last_os_error());
        }
        Ok(Self { ptr })
    }

    /// Open existing shm queue
    pub fn open(name: impl AsRef<Path>) -> Result<Self> {
        let name = CString::new(name.as_ref().to_string_lossy().as_bytes())?;
        let ptr = unsafe { ffi::kz_open(name.as_ptr()) };
        if ptr.is_null() {
            return Err(Error::last_os_error());
        }
        Ok(Self { ptr })
    }

    /// Queue name
    pub fn name(&self) -> Cow<'_, str> {
        unsafe { CStr::from_ptr(ffi::kz_name(self.ptr)) }.to_string_lossy()
    }

    /// Queue identifier
    pub fn ident(&self) -> u32 {
        unsafe { ffi::kz_ident(self.ptr) }
    }

    /// current process id
    pub fn pid(&self) -> i32 {
        unsafe { ffi::kz_pid(self.ptr) }
    }

    /// used bytess in queue
    pub fn used(&self) -> usize {
        unsafe { ffi::kz_used(self.ptr) }
    }

    /// total bytes in queue
    pub fn size(&self) -> usize {
        unsafe { ffi::kz_size(self.ptr) }
    }

    /// get owner pid of queue (sender, receiver)
    pub fn owner(&self) -> (i32, i32) {
        let mut sender = 0;
        let mut receiver = 0;
        unsafe { ffi::kz_owner(self.ptr, &mut sender, &mut receiver) };
        (sender, receiver)
    }

    /// set owner pid of queue
    pub fn set_owner(&mut self, sender: Option<i32>, receiver: Option<i32>) {
        unsafe {
            ffi::kz_set_owner(
                self.ptr,
                sender.unwrap_or(-1),
                receiver.unwrap_or(-1),
            )
        }
    }

    /// try to push data, return the push context if succeed
    pub fn try_push(&mut self, len: usize) -> Result<PushContext<'_>> {
        let mut ctx = MaybeUninit::uninit();
        match unsafe { ffi::kz_try_push(self.ptr, ctx.as_mut_ptr(), len) } {
            ffi::KZ_OK => Ok(PushContext {
                raw: unsafe { ctx.assume_init() },
                _marker: std::marker::PhantomData,
            }),
            code => Err(error_from_raw(code)),
        }
    }

    /// push data, blocking when queue is full
    pub fn push(&mut self, len: usize) -> Result<PushContext<'_>> {
        let mut ctx = MaybeUninit::uninit();
        match unsafe { ffi::kz_push(self.ptr, ctx.as_mut_ptr(), len) } {
            ffi::KZ_OK => Ok(PushContext {
                raw: unsafe { ctx.assume_init() },
                _marker: std::marker::PhantomData,
            }),
            code => Err(error_from_raw(code)),
        }
    }

    /// push data, blocking when queue is full with timeout
    pub fn push_until(
        &mut self,
        len: usize,
        millis: u32,
    ) -> Result<PushContext<'_>> {
        let mut ctx = MaybeUninit::uninit();
        match unsafe {
            ffi::kz_push_until(self.ptr, ctx.as_mut_ptr(), len, millis as i32)
        } {
            ffi::KZ_OK => Ok(PushContext {
                raw: unsafe { ctx.assume_init() },
                _marker: std::marker::PhantomData,
            }),
            code => Err(error_from_raw(code)),
        }
    }

    /// try to pop data, return the pop context if succeed
    pub fn try_pop(&mut self) -> Result<PopContext<'_>> {
        let mut ctx = MaybeUninit::uninit();
        match unsafe { ffi::kz_try_pop(self.ptr, ctx.as_mut_ptr()) } {
            ffi::KZ_OK => Ok(PopContext {
                raw: unsafe { ctx.assume_init() },
                _marker: std::marker::PhantomData,
            }),
            code => Err(error_from_raw(code)),
        }
    }

    /// pop data, blocking when queue is empty
    pub fn pop(&mut self) -> Result<PopContext<'_>> {
        let mut ctx = MaybeUninit::uninit();
        match unsafe { ffi::kz_pop(self.ptr, ctx.as_mut_ptr()) } {
            ffi::KZ_OK => Ok(PopContext {
                raw: unsafe { ctx.assume_init() },
                _marker: std::marker::PhantomData,
            }),
            code => Err(error_from_raw(code)),
        }
    }

    /// pop data, blocking when queue is empty with timeout
    pub fn pop_until(&mut self, millis: u32) -> Result<PopContext<'_>> {
        let mut ctx = MaybeUninit::uninit();
        match unsafe {
            ffi::kz_pop_until(self.ptr, ctx.as_mut_ptr(), millis as i32)
        } {
            ffi::KZ_OK => Ok(PopContext {
                raw: unsafe { ctx.assume_init() },
                _marker: std::marker::PhantomData,
            }),
            code => Err(error_from_raw(code)),
        }
    }
}

/// Push context used to copy data into queue
pub struct PushContext<'a> {
    raw: ffi::kz_PushContext,
    _marker: std::marker::PhantomData<&'a ()>,
}

unsafe impl Send for PushContext<'_> {}

impl PushContext<'_> {
    /// Returns a mutable reference to the write buffer.
    pub fn buffer_mut(&mut self) -> BytesMut<'_> {
        unsafe fn slice_from_raw<'a>(p: *mut i8, len: usize) -> &'a mut [u8] {
            if len == 0 {
                &mut []
            } else {
                unsafe { slice::from_raw_parts_mut(p.cast(), len) }
            }
        }
        let mut len1: usize = 0;
        let mut len2: usize = 0;
        unsafe {
            let p1 = ffi::kz_push_buffer(&mut self.raw, 0, &mut len1);
            let p2 = ffi::kz_push_buffer(&mut self.raw, 1, &mut len2);
            BytesMut::new((slice_from_raw(p1, len1), slice_from_raw(p2, len2)))
        }
    }

    /// Commit copied data with actual length
    pub fn commit(mut self, len: usize) -> Result<()> {
        match unsafe { ffi::kz_push_commit(&mut self.raw, len) } {
            ffi::KZ_OK => Ok(()),
            code => Err(error_from_raw(code)),
        }
    }
}

/// Pop context used to copy data from queue
pub struct PopContext<'a> {
    raw: ffi::kz_PopContext,
    _marker: std::marker::PhantomData<&'a ()>,
}

unsafe impl Send for PopContext<'_> {}

impl PopContext<'_> {
    /// Returns a reference to the read buffer.
    #[inline]
    pub fn buffer(&self) -> Bytes<'_> {
        unsafe fn slice_from_raw<'a>(p: *const i8, len: usize) -> &'a [u8] {
            if len == 0 {
                &[]
            } else {
                unsafe { slice::from_raw_parts(p.cast(), len) }
            }
        }
        let mut len1 = 0usize;
        let mut len2 = 0usize;
        unsafe {
            let p1 = ffi::kz_pop_buffer(&self.raw, 0, &mut len1);
            let p2 = ffi::kz_pop_buffer(&self.raw, 1, &mut len2);
            Bytes::new((slice_from_raw(p1, len1), slice_from_raw(p2, len2)))
        }
    }

    /// comfirm data has been consumed
    #[inline]
    pub fn commit(mut self) {
        unsafe { ffi::kz_pop_commit(&mut self.raw) }
    }
}

fn error_from_raw(code: i32) -> Error {
    match code {
        ffi::KZ_FAIL => Error::last_os_error(),
        ffi::KZ_CLOSED => Error::new(ErrorKind::BrokenPipe, "KZ_CLOSED"),
        ffi::KZ_INVALID => Error::new(ErrorKind::InvalidInput, "KZ_INVALID"),
        ffi::KZ_TOOBIG => Error::new(ErrorKind::FileTooLarge, "KZ_TOOBIG"),
        ffi::KZ_BUSY => Error::new(ErrorKind::WouldBlock, "KZ_BUSY"),
        ffi::KZ_TIMEOUT => Error::new(ErrorKind::TimedOut, "KZ_TIMEOUT"),
        _ => Error::other(format!("Unknown error({})", code)),
    }
}
