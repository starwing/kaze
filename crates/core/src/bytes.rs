use bytes::{buf::UninitSlice, Buf, BufMut};
use std::fmt::Display;

#[derive(Clone)]
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
        if self.pos == 0 {
            self.slice
        } else if self.pos >= self.slice.0.len() {
            (&self.slice.1[self.pos - self.slice.0.len()..], &[])
        } else {
            (&self.slice.0[self.pos..], &self.slice.1[..])
        }
    }
}

impl Display for Bytes<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bytes({}, pos={})", self.len(), self.pos)?;
        write!(f, "[{}]", String::from_utf8_lossy(self.slice.0))?;
        write!(f, "[{}]", String::from_utf8_lossy(self.slice.1))?;
        Ok(())
    }
}

impl<'a> Buf for Bytes<'a> {
    fn remaining(&self) -> usize {
        self.len().saturating_sub(self.pos)
    }

    fn chunk(&self) -> &[u8] {
        assert!(self.pos <= self.len());
        if self.pos >= self.slice.0.len() {
            &self.slice.1[self.pos - self.slice.0.len()..]
        } else {
            &self.slice.0[self.pos..]
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
        if self.pos == 0 {
            (&mut self.slice.0, &mut self.slice.1)
        } else if self.pos >= self.slice.0.len() {
            (&mut self.slice.1[self.pos - self.slice.0.len()..], &mut [])
        } else {
            (&mut self.slice.0[self.pos..], &mut self.slice.1[..])
        }
    }
}

unsafe impl BufMut for BytesMut<'_> {
    fn remaining_mut(&self) -> usize {
        self.len().saturating_sub(self.pos)
    }

    fn chunk_mut(&mut self) -> &mut UninitSlice {
        assert!(self.pos <= self.len());
        UninitSlice::new(if self.pos >= self.slice.0.len() {
            &mut self.slice.1[self.pos - self.slice.0.len()..]
        } else {
            &mut self.slice.0[self.pos..]
        })
    }

    unsafe fn advance_mut(&mut self, cnt: usize) {
        assert!(self.pos + cnt <= self.len());
        self.pos += cnt;
    }
}
