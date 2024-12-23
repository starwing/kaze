use std::io;
use std::path::PathBuf;

use crate::ringbuf::{ReceivedData, Receiver, RingBuffer, Sender};

#[cfg(target_os = "windows")]
#[path = "shm_windows.rs"]
mod imp;

#[cfg(not(target_os = "windows"))]
#[path = "shm_unix.rs"]
mod imp;

/// Shared memory object.
pub struct Shm {
    imp: imp::Shm,
    netside: RingBuffer,
    hostside: RingBuffer,
}

impl Shm {
    /// Create a new shared memory object.
    pub fn new(
        filename: PathBuf,
        ident: u32,
        netbuf: usize,
        hostbuf: usize,
    ) -> io::Result<Self> {
        let netsize = RingBuffer::requested_size(netbuf);
        let hostsize = RingBuffer::requested_size(hostbuf);
        let size = size_of::<ShmHdr>() + netsize + hostsize;
        let imp = imp::Shm::new(filename, size)?;
        let shm = Self::open_raw(imp, netsize, hostsize);
        shm.hdr_mut().init(ident, netsize, hostsize);
        Ok(shm)
    }

    /// Open an existing shared memory object.
    pub fn open(filename: PathBuf) -> io::Result<Self> {
        let imp = imp::Shm::open(filename)?;
        if imp.len() <= size_of::<ShmHdr>() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Shared memory is too small",
            ));
        }
        // SAFETY: size is enough for ShmHdr.
        let hdr = unsafe { imp.as_ptr().cast::<ShmHdr>().as_mut() };
        if hdr.size as usize != imp.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Shared memory size mismatch",
            ));
        }

        if hdr.host_pid != 0 {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Shared memory is already in use",
            ));
        }
        hdr.host_pid = std::process::id() as u32;

        Ok(Self::open_raw(
            imp,
            hdr.netside_size as usize,
            hdr.hostside_size as usize,
        ))
    }

    /// split the shared memory object into sender and receiver.
    pub fn split(&mut self) -> (Sender<'_>, Receiver<'_>) {
        let hdr = self.hdr();
        let (netsend, netrecv) = self.netside.split();
        let (hostsend, hostrecv) = self.hostside.split();
        if hdr.is_sdeicar() {
            (hostsend, netrecv)
        } else if hdr.is_host() {
            (netsend, hostrecv)
        } else {
            panic!("Shared memory is in invalid state");
        }
    }

    pub unsafe fn try_push(&self, data: &[u8]) -> bool {
        self.netside.sender().try_push(data)
    }

    pub unsafe fn push(&self, data: &[u8]) {
        self.netside.sender().push(data)
    }

    pub unsafe fn try_pop(&self) -> Option<ReceivedData<'static>> {
        self.hostside.receiver().try_pop_static()
    }

    pub unsafe fn pop(&self) -> ReceivedData<'static> {
        self.hostside.receiver().pop_static()
    }

    fn open_raw(imp: imp::Shm, netsize: usize, hostsize: usize) -> Self {
        let hdr = imp.as_ptr().cast::<ShmHdr>();

        // SAFETY: size is enough for two RingBuffers.
        let netside_mem = unsafe { hdr.add(1) }.cast::<u8>();

        // SAFETY: size is enough for two RingBuffers.
        let hostside_mem = unsafe { netside_mem.add(netsize) };

        let netside = RingBuffer::new(netside_mem, netsize);
        let hostside = RingBuffer::new(hostside_mem, hostsize);

        Self {
            imp,
            netside,
            hostside,
        }
    }

    fn hdr<'a, 'b>(&'a self) -> &'b ShmHdr {
        // SAFETY: size is enough for ShmHdr.
        unsafe { self.imp.as_ptr().cast::<ShmHdr>().as_ref() }
    }

    fn hdr_mut<'a, 'b>(&'a self) -> &'b mut ShmHdr {
        // SAFETY: size is enough for ShmHdr.
        unsafe { self.imp.as_ptr().cast::<ShmHdr>().as_mut() }
    }
}

struct ShmHdr {
    size: u32,          // Size of the shared memory. 4GB max.
    sidecar_ident: u32, // Sidecar process identifier.
    sidecar_pid: u32,   // Sidecar process id.
    host_pid: u32,      // Host process id.
    netside_size: u32,  // Size of the net side buffer.
    hostside_size: u32, // Size of the host side buffer.
}

impl ShmHdr {
    fn init(&mut self, ident: u32, netsize: usize, hostsize: usize) {
        self.size = (size_of::<Self>() + netsize + hostsize) as u32;
        self.sidecar_ident = ident;
        self.sidecar_pid = std::process::id() as u32;
        self.host_pid = 0;
        self.netside_size = netsize as u32;
        self.hostside_size = hostsize as u32;
    }

    #[inline]
    fn is_sdeicar(&self) -> bool {
        self.sidecar_pid == std::process::id() as u32
    }

    #[inline]
    fn is_host(&self) -> bool {
        self.host_pid == std::process::id() as u32
    }
}
