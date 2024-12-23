use core::slice;
use std::sync::atomic::Ordering;
use std::{ptr::NonNull, sync::atomic::AtomicU32};

/// A ring buffer that uses shared memory for inter-process communication
///
/// The ring buffer is implemented using a shared memory object and a futex
/// Layout: [hdr][data]
///              ^ data here
///
/// The header contains the size of the data buffer, the head and tail pointers,
/// and the number of bytes used and needed. The data buffer contains the actual
/// data that is enqueued and dequeued.
///
/// The data buffer is a circular buffer, fill with the data chunks. data chunks
/// prefixed by a 4-byte size field, followed by the actual data. The size field
/// are aligned to 4 bytes.
pub struct RingBuffer(RBData);

// the prefix size of the data chunk. used to storage the size of the data chunk
const PS: usize = size_of::<u32>();

impl RingBuffer {
    /// Returns the size of the ring buffer
    pub fn requested_size(size: usize) -> usize {
        size + size_of::<RBHdr>()
    }

    /// Create a new ring buffer with the given shared memory object and size
    pub fn new(shm: impl Into<NonNull<u8>>, size: usize) -> Self {
        let mut hdr: NonNull<RBHdr> = shm.into().cast();

        assert!(size > size_of::<RBHdr>() + PS);
        assert!(is_aligned_to(size - size_of::<RBHdr>(), PS));
        // SAFETY: size is greater than the size of the header.
        unsafe { hdr.as_mut() }.init(size - size_of::<RBHdr>());

        // SAFETY: size is greater than the size of the header.
        let data = unsafe { hdr.add(1) }.cast();
        debug_assert!(data.align_offset(size_of::<u32>()) == 0);
        Self(RBData(data))
    }

    /// get receiver and sender for the ring buffer
    pub fn split<'a>(&'a mut self) -> (Sender<'a>, Receiver<'a>) {
        (
            Sender {
                data: self.0,
                _marker: std::marker::PhantomData,
            },
            Receiver {
                data: self.0,
                _marker: std::marker::PhantomData,
            },
        )
    }

    /// get sender for the ring buffer. caller must ensure the lifetime of the sender
    ///
    /// # Safety
    /// There must be only one sender for the ring buffer
    pub unsafe fn sender(&self) -> Sender<'static> {
        Sender {
            data: self.0,
            _marker: std::marker::PhantomData,
        }
    }

    /// get receiver for the ring buffer. caller must ensure the lifetime of the receiver
    ///
    /// # Safety
    /// There must be only one receiver for the ring buffer
    pub unsafe fn receiver(&self) -> Receiver<'static> {
        Receiver {
            data: self.0,
            _marker: std::marker::PhantomData,
        }
    }
}

#[repr(C)]
struct RBHdr {
    size: u32,
    head: u32,
    tail: u32,
    used: AtomicU32,
    need: AtomicU32,
}

impl RBHdr {
    fn init(&mut self, size: usize) {
        debug_assert!(size <= i32::MAX as usize);
        self.size = size as u32;
        self.head = 0;
        self.tail = 0;
        self.used.store(0, Ordering::Relaxed);
        self.need.store(0, Ordering::Relaxed);
    }

    #[inline]
    fn size(&self) -> usize {
        self.size as usize
    }

    #[inline]
    fn used(&self) -> usize {
        self.used.load(Ordering::Acquire) as usize
    }

    #[inline]
    fn get_free_size(&self) -> usize {
        self.size() - self.used()
    }
}

#[derive(Clone, Copy, Debug)]
struct RBData(NonNull<u8>);

impl RBData {
    /// Returns the header of the ring buffer
    fn hdr(&self) -> &RBHdr {
        // SAFETY: the header is always present before the data
        unsafe { self.0.cast::<RBHdr>().sub(1).as_ref() }
    }

    /// Returns the mutable header of the ring buffer
    fn hdr_mut<'a, 'b>(&'a self) -> &'b mut RBHdr {
        // SAFETY: the header is always present before the data
        unsafe { self.0.cast::<RBHdr>().sub(1).as_mut() }
    }

    #[inline]
    fn slice(&self, start: usize, len: usize) -> &[u8] {
        debug_assert!(start + len <= self.hdr().size());
        // SAFETY: start and len are always within the bounds of the data
        unsafe { slice::from_raw_parts(self.0.add(start).as_ptr(), len) }
    }

    #[inline]
    fn slice_mut(&self, start: usize, len: usize) -> &mut [u8] {
        debug_assert!(start + len <= self.hdr().size());
        // SAFETY: start and len are always within the bounds of the data
        unsafe { slice::from_raw_parts_mut(self.0.add(start).as_mut(), len) }
    }
}

/// Sender side of the ring buffer
pub struct Sender<'a> {
    data: RBData,
    _marker: std::marker::PhantomData<&'a ()>,
}

unsafe impl Send for Sender<'_> {}

impl Sender<'_> {
    /// Enqueue data into the ring buffer
    ///
    /// Returns true if the data was successfully enqueued,
    /// and false when futex_wait is needed.
    pub fn try_push(&mut self, data: &[u8]) -> bool {
        let needed_space = get_aligned_size(PS + data.len(), PS);
        assert!(needed_space <= i32::MAX as usize);

        // check if there is enough space
        let hdr = self.data.hdr_mut();
        assert!(needed_space <= hdr.size());
        let free_space = hdr.get_free_size();
        if free_space < needed_space {
            let addition_needed = (needed_space - free_space) as u32;
            hdr.need.store(addition_needed, Ordering::Release);
            return false;
        }

        // do the actual enqueuing
        let (first, second) = self.get_free_slices(needed_space);
        first[..PS].copy_from_slice(&(data.len() as u32).to_le_bytes());
        if first.len() >= needed_space {
            first[PS..][..data.len()].copy_from_slice(data);
        } else {
            let first_size = first.len() - PS;
            first[PS..].copy_from_slice(&data[..first_size]);
            second[..data.len() - first_size]
                .copy_from_slice(&data[first_size..]);
        }

        // update the tail and used
        hdr.tail = (hdr.tail + needed_space as u32) % hdr.size;
        debug_assert!(is_aligned_to(hdr.tail as usize, PS));
        let old_used =
            hdr.used.fetch_add(needed_space as u32, Ordering::Release);
        if old_used == 0 {
            // wake up the other side
            atomic_wait::wake_one(&hdr.used);
        }

        true
    }

    /// Enqueue data into the ring buffer
    ///
    /// It may wait until there is enough space
    pub fn push(&mut self, data: &[u8]) {
        loop {
            // try to enqueue the data
            if self.try_push(data) {
                return;
            }

            // wait until there is enough space
            self.wait_need()
        }
    }

    fn get_free_slices(&mut self, size: usize) -> (&mut [u8], &mut [u8]) {
        let hdr = self.data.hdr();
        let tail = hdr.tail as usize;
        debug_assert!(tail < hdr.size());
        // SAFETY: after modulus, tail is always within the bounds of the data
        let first = self.data.slice_mut(tail, hdr.size() - tail);
        debug_assert!(first.len() > 0);
        if first.len() >= size {
            (&mut first[..size], &mut [])
        } else {
            // SAFETY: size is always less than the total size of buffer
            let second = self.data.slice_mut(0, size - first.len());
            (first, second)
        }
    }

    #[inline]
    fn wait_need(&mut self) {
        let need = &self.data.hdr().need;
        let need_value = need.load(Ordering::Acquire);
        atomic_wait::wait(need, need_value);
    }
}

/// Receiver side of the ring buffer
pub struct Receiver<'a> {
    data: RBData,
    _marker: std::marker::PhantomData<&'a ()>,
}

unsafe impl Send for Receiver<'_> {}

// getters
impl Receiver<'_> {
    /// Try to dequeue data from the ring buffer
    pub fn try_pop(&mut self) -> Option<ReceivedData<'_>> {
        self.try_pop_raw()
            .map(|(head, size)| self.new_received_data(head, size))
    }

    /// Dequeue data from the ring buffer
    ///
    /// It may wait until there is data available
    pub fn pop(&mut self) -> ReceivedData<'_> {
        let hdr = self.data.hdr_mut();
        loop {
            if let Some((head, size)) = self.try_pop_raw() {
                return self.new_received_data(head, size);
            }

            atomic_wait::wait(&hdr.used, 0);
        }
    }

    pub unsafe fn try_pop_static(&self) -> Option<ReceivedData<'static>> {
        self.try_pop_raw()
            .map(|(head, size)| self.new_received_data_static(head, size))
    }

    pub unsafe fn pop_static(&self) -> ReceivedData<'static> {
        let hdr = self.data.hdr_mut();
        loop {
            if let Some((head, size)) = self.try_pop_raw() {
                return self.new_received_data_static(head, size);
            }

            atomic_wait::wait(&hdr.used, 0);
        }
    }

    /// Returns the offset to the data and the size of the data
    fn try_pop_raw(&self) -> Option<(usize, usize)> {
        let hdr = self.data.hdr_mut();
        let used = hdr.used();

        if used == 0 {
            return None;
        }

        // extract size from buffers
        debug_assert!(used >= PS);
        let mut head = hdr.head as usize;
        debug_assert!(head < hdr.size());

        let first = self.data.slice(head, hdr.size() - head);
        debug_assert!(first.len() >= PS);
        let size = u32::from_le_bytes(first[..PS].try_into().unwrap());

        head += PS;
        if head == hdr.size() {
            head = 0
        }
        Some((head, size as usize))
    }

    fn new_received_data(&self, head: usize, size: usize) -> ReceivedData<'_> {
        ReceivedData {
            data: self.data,
            head,
            size,
            _marker: std::marker::PhantomData,
        }
    }

    unsafe fn new_received_data_static(
        &self,
        head: usize,
        size: usize,
    ) -> ReceivedData<'static> {
        ReceivedData {
            data: self.data,
            head,
            size,
            _marker: std::marker::PhantomData,
        }
    }
}

/// Received data from the ring buffer
#[derive(Debug)]
pub struct ReceivedData<'a> {
    data: RBData,
    head: usize,
    size: usize,
    _marker: std::marker::PhantomData<&'a ()>,
}

unsafe impl Send for ReceivedData<'_> {}

impl Drop for ReceivedData<'_> {
    fn drop(&mut self) {
        let hdr = self.data.hdr_mut();
        // update the head
        let head = self.head as u32;
        let commit_size = get_aligned_size(self.size, PS) as u32;
        hdr.head = (head + commit_size) % hdr.size;

        // update the used and need
        let total_size = commit_size + PS as u32;
        hdr.used.fetch_sub(total_size, Ordering::Release);
        let old_need = hdr.need.fetch_sub(total_size, Ordering::AcqRel);

        if (old_need as i32) < (total_size as i32) {
            // wake up the other side
            atomic_wait::wake_one(&hdr.need);
        }
    }
}

impl ReceivedData<'_> {
    /// Returns the data as a slice
    pub fn as_slice(&self) -> (&[u8], &[u8]) {
        let hdr = self.data.hdr();
        let first_len = hdr.size() - self.head;
        let first = self.data.slice(self.head, first_len);
        if first_len >= self.size {
            return (&first[..self.size], &[]);
        }
        (first, self.data.slice(0, self.size - first_len))
    }
}

/// Returns the aligned size of the given size.
fn get_aligned_size(size: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (size + align - 1) & !(align - 1)
}

/// Returns true if the given size is aligned to the given alignment.
fn is_aligned_to(size: usize, align: usize) -> bool {
    debug_assert!(align.is_power_of_two());
    size & (align - 1) == 0
}

#[test]
fn test_basic() {
    let mut data = [0u8; size_of::<RBHdr>() + 16];
    let mut rb = RingBuffer::new(
        unsafe { NonNull::new_unchecked(data.as_mut_ptr()) },
        data.len(),
    );

    let (mut send, mut recv) = rb.split();
    assert!(recv.try_pop().is_none());
    assert!(send.try_push(&[1, 2, 3]));
    let r = recv.try_pop();
    assert!(r.is_some());
    assert!(r.unwrap().as_slice().0 == [1, 2, 3]);
}

#[test]
fn test_full() {
    let mut data = [0u8; size_of::<RBHdr>() + 16];
    let mut rb = RingBuffer::new(
        unsafe { NonNull::new_unchecked(data.as_mut_ptr()) },
        data.len(),
    );

    let (mut send, mut recv) = rb.split();
    assert!(send.try_push(&[1, 2, 3])); // 8 bytes
    assert!(send.try_push(&[4, 5, 6])); // 16 bytes
    assert!(!send.try_push(&[7, 8, 9])); // failed

    let r1 = recv.try_pop();
    assert!(r1.is_some());
    assert!(r1.unwrap().as_slice().0 == [1, 2, 3]);

    let r2 = recv.try_pop();
    assert!(r2.is_some());
    assert!(r2.unwrap().as_slice().0 == [4, 5, 6]);

    assert!(recv.try_pop().is_none());
}

#[test]
fn test_sync() {
    let mut data = [0u8; size_of::<RBHdr>() + 16];
    let mut rb = RingBuffer::new(
        unsafe { NonNull::new_unchecked(data.as_mut_ptr()) },
        data.len(),
    );

    let (mut send, mut recv) = rb.split();
    std::thread::scope(|s| {
        s.spawn(move || {
            println!("t1: start");
            let r = recv.pop();
            println!("t1: after pop");
            assert!(r.as_slice().0 == [1, 2, 3]);
        });

        // Sleep for a short duration to ensure t1 is blocked on pop
        println!("main: before sleep");
        std::thread::sleep(std::time::Duration::from_millis(100));
        println!("main: before push");
        send.push(&[1, 2, 3]);
        println!("main: after push");
    });
}

#[test]
fn test_push_pop_order() {
    let mut data = [0u8; size_of::<RBHdr>() + 32];
    let mut rb = RingBuffer::new(
        unsafe { NonNull::new_unchecked(data.as_mut_ptr()) },
        data.len(),
    );

    let (mut send, mut recv) = rb.split();
    assert!(send.try_push(&[1, 2, 3]));
    assert!(send.try_push(&[4, 5, 6]));
    let r1 = recv.try_pop();
    assert!(r1.is_some());
    assert!(r1.unwrap().as_slice().0 == [1, 2, 3]);

    let r2 = recv.try_pop();
    assert!(r2.is_some());
    println!("{:?}", r2);
    assert!(r2.unwrap().as_slice().0 == [4, 5, 6]);
}
