use bytes::{buf::UninitSlice, Buf, BufMut};
use std::{fmt::Debug, slice};

#[derive(Clone, Copy)]
pub struct Bytes<'a> {
    slice: (&'a [u8], &'a [u8]),
    pos: usize,
}

impl Bytes<'_> {
    pub fn new<'a>(slice: (&'a [u8], &'a [u8])) -> Bytes<'a> {
        Bytes::<'a> { slice, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn len(&self) -> usize {
        self.slice.0.len() + self.slice.1.len()
    }

    pub fn rewind(&mut self) {
        self.pos = 0;
    }

    pub fn set_pos(&mut self, pos: usize) {
        assert!(pos <= self.len());
        self.pos = pos;
    }

    pub fn as_slice(&self) -> (&[u8], &[u8]) {
        if self.pos < self.slice.0.len() {
            (&self.slice.0[self.pos..], &self.slice.1[..])
        } else {
            (&self.slice.1[self.pos - self.slice.0.len()..], &[])
        }
    }
}

impl Debug for Bytes<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bytes")
            .field("len", &self.len())
            .field("pos", &self.pos)
            .field("slice.0", &String::from_utf8_lossy(self.slice.0))
            .field("slice.1", &String::from_utf8_lossy(self.slice.1))
            .finish()
    }
}

impl<'a> Buf for Bytes<'a> {
    fn remaining(&self) -> usize {
        self.len().saturating_sub(self.pos)
    }

    fn chunk(&self) -> &[u8] {
        assert!(self.pos <= self.len());
        if self.pos < self.slice.0.len() {
            &self.slice.0[self.pos..]
        } else {
            &self.slice.1[self.pos - self.slice.0.len()..]
        }
    }

    fn advance(&mut self, cnt: usize) {
        assert!(self.pos + cnt <= self.len());
        self.pos += cnt;
    }
}

pub struct BytesMut<'a> {
    slice: (&'a mut [u8], &'a mut [u8]),
    pos: usize,
}

impl BytesMut<'_> {
    pub fn new<'a>(slice: (&'a mut [u8], &'a mut [u8])) -> BytesMut<'a> {
        BytesMut::<'a> { slice, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn len(&self) -> usize {
        self.slice.0.len() + self.slice.1.len()
    }

    pub fn rewind(&mut self) {
        self.pos = 0;
    }

    pub fn as_slice(&mut self) -> (&mut [u8], &mut [u8]) {
        if self.pos < self.slice.0.len() {
            (&mut self.slice.0[self.pos..], &mut self.slice.1[..])
        } else {
            (&mut self.slice.1[self.pos - self.slice.0.len()..], &mut [])
        }
    }
}

impl Debug for BytesMut<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BytesMut")
            .field("len", &self.len())
            .field("pos", &self.pos)
            .field("slice.0", &String::from_utf8_lossy(self.slice.0))
            .field("slice.1", &String::from_utf8_lossy(self.slice.1))
            .finish()
    }
}

unsafe impl BufMut for BytesMut<'_> {
    fn remaining_mut(&self) -> usize {
        self.len().saturating_sub(self.pos)
    }

    fn chunk_mut(&mut self) -> &mut UninitSlice {
        assert!(self.pos <= self.len());
        UninitSlice::new(if self.pos < self.slice.0.len() {
            &mut self.slice.0[self.pos..]
        } else {
            &mut self.slice.1[self.pos - self.slice.0.len()..]
        })
    }

    unsafe fn advance_mut(&mut self, cnt: usize) {
        assert!(self.pos + cnt <= self.len());
        self.pos += cnt;
    }
}

#[derive(Clone, Copy)]
pub struct BytesUnsafe {
    p1: *const u8,
    len1: usize,
    p2: *const u8,
    len2: usize,
    pos: usize,
}

unsafe impl Send for BytesUnsafe {}
unsafe impl Sync for BytesUnsafe {}

impl BytesUnsafe {
    pub fn new(
        p1: *const u8,
        len1: usize,
        p2: *const u8,
        len2: usize,
    ) -> BytesUnsafe {
        BytesUnsafe {
            p1,
            len1,
            p2,
            len2,
            pos: 0,
        }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn len(&self) -> usize {
        self.len1 + self.len2
    }

    pub fn rewind(&mut self) {
        self.pos = 0;
    }

    pub fn set_pos(&mut self, pos: usize) {
        assert!(pos <= self.len());
        self.pos = pos;
    }

    pub fn as_slice(&self) -> (&[u8], &[u8]) {
        let (s1, s2) = unsafe {
            (
                slice::from_raw_parts(self.p1, self.len1),
                slice::from_raw_parts(self.p2, self.len2),
            )
        };
        if self.pos < self.len1 {
            (&s1[self.pos..], &s2)
        } else {
            (&s2[..self.pos - self.len1], &[])
        }
    }
}

impl Debug for BytesUnsafe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (s1, s2) = unsafe {
            (
                slice::from_raw_parts(self.p1, self.len1),
                slice::from_raw_parts(self.p2, self.len2),
            )
        };
        f.debug_struct("BytesUnsafe")
            .field("len", &(s1.len() + s2.len()))
            .field("pos", &self.pos)
            .field("slice.0", &String::from_utf8_lossy(s1))
            .field("slice.1", &String::from_utf8_lossy(s2))
            .finish()
    }
}

impl Buf for BytesUnsafe {
    fn remaining(&self) -> usize {
        self.len().saturating_sub(self.pos)
    }

    fn chunk(&self) -> &[u8] {
        assert!(self.pos <= self.len());
        let (s1, s2) = unsafe {
            (
                slice::from_raw_parts(self.p1, self.len1),
                slice::from_raw_parts(self.p2, self.len2),
            )
        };
        if self.pos < self.len1 {
            &s1[self.pos..]
        } else {
            &s2[self.pos - self.len1..]
        }
    }

    fn advance(&mut self, cnt: usize) {
        assert!(self.pos + cnt <= self.len());
        self.pos += cnt;
    }
}
