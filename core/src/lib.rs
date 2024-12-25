use std::{ffi::CString, io, mem::MaybeUninit, slice};

mod ffi;

pub struct KazeState {
    ptr: *mut ffi::kz_State,
}

unsafe impl Send for KazeState {}

impl Drop for KazeState {
    fn drop(&mut self) {
        unsafe { ffi::kz_delete(self.ptr) }
    }
}

impl KazeState {
    pub fn unlink(name: &str) -> io::Result<()> {
        let name = CString::new(name)?;
        match unsafe { ffi::kz_unlink(name.as_ptr()) } {
            ffi::KZ_OK => Ok(()),
            code => Err(io::Error::from_raw_os_error(code as i32)),
        }
    }

    pub fn new(name: &str, ident: u32, bufsize: usize) -> io::Result<Self> {
        let name = CString::new(name)?;
        let ptr = unsafe { ffi::kz_new(name.as_ptr(), ident, bufsize) };
        if ptr.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { ptr })
    }

    pub fn open(name: &str) -> io::Result<Self> {
        let name = CString::new(name)?;
        let ptr = unsafe { ffi::kz_open(name.as_ptr()) };
        if ptr.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { ptr })
    }

    pub fn name(&self) -> &str {
        unsafe {
            std::ffi::CStr::from_ptr(ffi::kz_name(self.ptr))
                .to_str()
                .unwrap()
        }
    }

    pub fn ident(&self) -> u32 {
        unsafe { ffi::kz_ident(self.ptr) }
    }

    pub fn pid(&self) -> i32 {
        unsafe { ffi::kz_pid(self.ptr) }
    }

    pub fn used(&self) -> usize {
        unsafe { ffi::kz_used(self.ptr) }
    }

    pub fn size(&self) -> usize {
        unsafe { ffi::kz_size(self.ptr) }
    }

    pub fn owner(&self) -> (i32, i32) {
        let mut sender = 0;
        let mut receiver = 0;
        unsafe { ffi::kz_owner(self.ptr, &mut sender, &mut receiver) };
        (sender, receiver)
    }

    pub fn set_owner(&mut self, sender: Option<i32>, receiver: Option<i32>) {
        unsafe {
            ffi::kz_set_owner(
                self.ptr,
                sender.unwrap_or(-1),
                receiver.unwrap_or(-1),
            )
        }
    }

    pub fn try_push(&mut self, len: usize) -> Result<PushContext<'_>> {
        let mut ctx = MaybeUninit::uninit();
        match unsafe { ffi::kz_try_push(self.ptr, ctx.as_mut_ptr(), len) } {
            ffi::KZ_OK => Ok(PushContext {
                raw: unsafe { ctx.assume_init() },
                _marker: std::marker::PhantomData,
            }),
            code => Err(Error { code }),
        }
    }

    pub fn push(&mut self, len: usize) -> Result<PushContext<'_>> {
        let mut ctx = MaybeUninit::uninit();
        match unsafe { ffi::kz_push(self.ptr, ctx.as_mut_ptr(), len) } {
            ffi::KZ_OK => Ok(PushContext {
                raw: unsafe { ctx.assume_init() },
                _marker: std::marker::PhantomData,
            }),
            code => Err(Error { code }),
        }
    }

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
            code => Err(Error { code }),
        }
    }

    pub fn try_pop(&mut self) -> Result<PopContext<'_>> {
        let mut ctx = MaybeUninit::uninit();
        match unsafe { ffi::kz_try_pop(self.ptr, ctx.as_mut_ptr()) } {
            ffi::KZ_OK => Ok(PopContext {
                raw: unsafe { ctx.assume_init() },
                _marker: std::marker::PhantomData,
            }),
            code => Err(Error { code }),
        }
    }

    pub fn pop(&mut self) -> Result<PopContext<'_>> {
        let mut ctx = MaybeUninit::uninit();
        match unsafe { ffi::kz_pop(self.ptr, ctx.as_mut_ptr()) } {
            ffi::KZ_OK => Ok(PopContext {
                raw: unsafe { ctx.assume_init() },
                _marker: std::marker::PhantomData,
            }),
            code => Err(Error { code }),
        }
    }

    pub fn pop_until(&mut self, millis: u32) -> Result<PopContext<'_>> {
        let mut ctx = MaybeUninit::uninit();
        match unsafe {
            ffi::kz_pop_until(self.ptr, ctx.as_mut_ptr(), millis as i32)
        } {
            ffi::KZ_OK => Ok(PopContext {
                raw: unsafe { ctx.assume_init() },
                _marker: std::marker::PhantomData,
            }),
            code => Err(Error { code }),
        }
    }
}

pub struct PushContext<'a> {
    raw: ffi::kz_PushContext,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl PushContext<'_> {
    pub fn buffer_mut(&mut self) -> (&mut [u8], &mut [u8]) {
        unsafe fn slice_from_raw<'a>(p: *mut i8, len: usize) -> &'a mut [u8] {
            if len == 0 {
                &mut []
            } else {
                unsafe { slice::from_raw_parts_mut(p.cast(), len) }
            }
        }
        unsafe {
            let mut len1: usize = 0;
            let p1 = ffi::kz_push_buffer(&mut self.raw, 0, &mut len1);
            let mut len2: usize = 0;
            let p2 = ffi::kz_push_buffer(&mut self.raw, 1, &mut len2);
            (slice_from_raw(p1, len1), slice_from_raw(p2, len2))
        }
    }

    pub fn commit(mut self, len: usize) -> Result<()> {
        match unsafe { ffi::kz_push_commit(&mut self.raw, len) } {
            ffi::KZ_OK => Ok(()),
            code => Err(Error { code }),
        }
    }
}

pub struct PopContext<'a> {
    raw: ffi::kz_PopContext,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl PopContext<'_> {
    #[inline]
    pub fn buffer(&self) -> (&[u8], &[u8]) {
        unsafe fn slice_from_raw<'a>(p: *const i8, len: usize) -> &'a [u8] {
            if len == 0 {
                &[]
            } else {
                unsafe { slice::from_raw_parts(p.cast(), len) }
            }
        }
        let mut len1 = 0usize;
        let p1 = unsafe { ffi::kz_pop_buffer(&self.raw, 0, &mut len1) };
        let mut len2 = 0usize;
        let p2 = unsafe { ffi::kz_pop_buffer(&self.raw, 1, &mut len2) };
        unsafe { (slice_from_raw(p1, len1), slice_from_raw(p2, len2)) }
    }

    #[inline]
    pub fn commit(mut self) {
        unsafe { ffi::kz_pop_commit(&mut self.raw) }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub struct Error {
    code: i32,
}

impl std::error::Error for Error {}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "KazeError({})", self)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.code > ffi::KZ_LASTERR {
            write!(
                f,
                "{}",
                match self.code {
                    ffi::KZ_OK => "KZ_OK",
                    ffi::KZ_FAIL => "KZ_FAIL",
                    ffi::KZ_CLOSED => "KZ_CLOSED",
                    ffi::KZ_INVALID => "KZ_INVALID",
                    ffi::KZ_TOOBIG => "KZ_TOOBIG",
                    ffi::KZ_BUSY => "KZ_BUSY",
                    ffi::KZ_TIMEOUT => "KZ_TIMEOUT",
                    _ => unreachable!(),
                }
            )
        } else {
            write!(f, "Code({})", self.code)
        }
    }
}
