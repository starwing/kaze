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

    pub fn cleanup_host(name: &str) -> io::Result<()> {
        let name = CString::new(name)?;
        match unsafe { ffi::kz_cleanup_host(name.as_ptr()) } {
            ffi::KZ_OK => Ok(()),
            code => Err(io::Error::from_raw_os_error(code as i32)),
        }
    }

    pub fn new(
        name: &str,
        ident: u32,
        netbuf: usize,
        hostbuf: usize,
    ) -> io::Result<Self> {
        let name = CString::new(name)?;
        let ptr =
            unsafe { ffi::kz_new(name.as_ptr(), ident, netbuf, hostbuf) };
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

    pub unsafe fn dup(&self) -> KazeState {
        KazeState { ptr: self.ptr }
    }

    pub fn name(&self) -> &str {
        unsafe {
            std::ffi::CStr::from_ptr(ffi::kz_name(self.ptr))
                .to_str()
                .unwrap()
        }
    }

    pub fn is_sidecar(&self) -> bool {
        unsafe { ffi::kz_is_sidecar(self.ptr) != 0 }
    }

    pub fn is_host(&self) -> bool {
        unsafe { ffi::kz_is_host(self.ptr) != 0 }
    }

    pub fn try_push(&mut self, data: &[u8]) -> Result<bool> {
        match unsafe {
            ffi::kz_try_push(self.ptr, data.as_ptr().cast(), data.len())
        } {
            ffi::KZ_OK => Ok(true),
            ffi::KZ_BUSY => Ok(false),
            code => Err(Error { code }),
        }
    }

    pub fn push(&mut self, data: &[u8]) -> Result<()> {
        match unsafe {
            ffi::kz_push(self.ptr, data.as_ptr().cast(), data.len())
        } {
            ffi::KZ_OK => Ok(()),
            code => Err(Error { code }),
        }
    }

    pub fn push_until(&mut self, data: &[u8], millis: u32) {
        match unsafe {
            ffi::kz_push_until(
                self.ptr,
                data.as_ptr().cast(),
                data.len(),
                millis as i32,
            )
        } {
            ffi::KZ_OK => (),
            code => panic!("kz_push_until failed: {}", code),
        }
    }

    pub fn try_pop(&mut self) -> Result<ReceivedData<'_>> {
        let mut data = MaybeUninit::uninit();
        match unsafe { ffi::kz_try_pop(self.ptr, data.as_mut_ptr()) } {
            ffi::KZ_OK => Ok(ReceivedData {
                raw: unsafe { data.assume_init() },
                _marker: std::marker::PhantomData,
            }),
            code => Err(Error { code }),
        }
    }

    pub fn pop(&mut self) -> Result<ReceivedData<'_>> {
        let mut data = MaybeUninit::uninit();
        match unsafe { ffi::kz_pop(self.ptr, data.as_mut_ptr()) } {
            ffi::KZ_OK => Ok(ReceivedData {
                raw: unsafe { data.assume_init() },
                _marker: std::marker::PhantomData,
            }),
            code => Err(Error { code }),
        }
    }

    pub fn pop_until(&mut self, millis: u32) -> Result<ReceivedData<'_>> {
        let mut data = MaybeUninit::uninit();
        match unsafe {
            ffi::kz_pop_until(self.ptr, data.as_mut_ptr(), millis as i32)
        } {
            ffi::KZ_OK => Ok(ReceivedData {
                raw: unsafe { data.assume_init() },
                _marker: std::marker::PhantomData,
            }),
            code => Err(Error { code }),
        }
    }
}

pub struct ReceivedData<'a> {
    raw: ffi::kz_ReceivedData,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl Drop for ReceivedData<'_> {
    #[inline]
    fn drop(&mut self) {
        unsafe { ffi::kz_data_free(&mut self.raw) }
    }
}

impl ReceivedData<'_> {
    #[inline]
    pub fn as_slices(&self) -> (&[u8], &[u8]) {
        let cnt = unsafe { ffi::kz_data_count(&self.raw) };
        if cnt == 1 {
            let mut size = 0usize;
            let data = unsafe { ffi::kz_data_part(&self.raw, 0, &mut size) };
            return unsafe { (slice::from_raw_parts(data.cast(), size), &[]) };
        }
        let mut size = 0usize;
        let data = unsafe { ffi::kz_data_part(&self.raw, 0, &mut size) };
        let mut size2 = 0usize;
        let data2 = unsafe { ffi::kz_data_part(&self.raw, 1, &mut size2) };
        unsafe {
            (
                slice::from_raw_parts(data.cast(), size),
                slice::from_raw_parts(data2.cast(), size2),
            )
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        unsafe { ffi::kz_data_len(&self.raw) }
    }
}

type Result<T, E = Error> = std::result::Result<T, E>;

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
