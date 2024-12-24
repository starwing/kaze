#![cfg(not(target_os = "windows"))]

use std::{
    io,
    mem::MaybeUninit,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::ffi::OsStrExt,
    },
    path::PathBuf,
    ptr::NonNull,
};

/// Shared memory object.
///
/// This object represents a shared memory object that can be used to share data
/// between processes.
pub struct Shm {
    fd: OwnedFd,
    name: PathBuf,
    mem: NonNull<u8>,
    size: usize,
    unlink: bool,
}

impl Drop for Shm {
    fn drop(&mut self) {
        unsafe {
            assert!(libc::munmap(self.mem.as_ptr().cast(), self.size) == 0);
            if self.unlink {
                libc::shm_unlink(pathbuf_to_c_char(&self.name));
            }
        }
    }
}

impl Shm {
    /// Create a new shared memory object.
    pub fn new(filename: PathBuf, size: usize) -> io::Result<Self> {
        use libc::{O_CREAT, O_EXCL, O_RDWR};
        unsafe { Self::open_raw(filename, size, O_CREAT | O_EXCL | O_RDWR) }
    }

    /// Open an existing shared memory object.
    pub fn open(filename: PathBuf) -> io::Result<Self> {
        unsafe { Self::open_raw(filename, 0, libc::O_RDWR) }
    }

    /// Mark the shared memory file will be unlinked when the object is dropped.
    pub fn mark_unlink(&mut self, unlink: bool) {
        self.unlink = unlink;
    }

    /// Get the memory address of the shared memory object.
    pub fn as_ptr(&self) -> NonNull<u8> {
        self.mem
    }

    /// Get the size of the memory area mapped by shared memory object.
    pub fn len(&self) -> usize {
        self.size
    }

    unsafe fn open_raw(
        name: PathBuf,
        size: usize,
        oflags: i32,
    ) -> io::Result<Self> {
        let fd = libc::shm_open(pathbuf_to_c_char(&name), oflags, 0);
        if fd == -1 {
            return Err(io::Error::last_os_error());
        }

        let fd = OwnedFd::from_raw_fd(fd);
        let stat = unsafe {
            let mut stat = MaybeUninit::uninit();
            if libc::fstat(fd.as_raw_fd(), stat.as_mut_ptr()) == -1 {
                return Err(io::Error::last_os_error());
            }
            stat.assume_init()
        };

        let size = if stat.st_size != 0 {
            stat.st_size as usize
        } else if libc::ftruncate(fd.as_raw_fd(), size as libc::off_t) == -1 {
            return Err(io::Error::last_os_error());
        } else {
            size
        };

        let mem = libc::mmap(
            std::ptr::null_mut(),
            stat.st_size as usize,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd.as_raw_fd(),
            0,
        );
        if mem == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }

        let mem = NonNull::new_unchecked(mem).cast();
        Ok(Self {
            fd,
            name,
            mem,
            size,
            unlink: false,
        })
    }
}

// only used on platforms that use UTF-8.
fn pathbuf_to_c_char(path: &PathBuf) -> *const libc::c_char {
    path.as_os_str().as_bytes().as_ptr() as *const libc::c_char
}
