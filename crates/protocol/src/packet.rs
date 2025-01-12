use std::{io::IoSlice, ptr::addr_of};

use anyhow::Result;

use prost::Message;
use tokio_util::bytes::{Buf, BufMut, BytesMut};

use crate::{decode_packet, hdr::RpcType, BufWrapper, Hdr, RetCode};

/// the Packet object can be used in anywhere
///
/// - decoded from network
/// - decoded from host shm queue
/// - send to network with write_vectored
/// - send to host shm queue with impl Buf
pub struct Packet<'a> {
    hdr_dirty: bool,
    hdr: Hdr,
    body: PacketBody<'a>,
}

#[derive(Clone)]
enum PacketBody<'a> {
    Empty,
    FromBuf(BytesMut),
    FromHost(kaze_core::Bytes<'a>),
    FromNode(BufWrapper<BytesMut>),
}

impl Buf for PacketBody<'_> {
    fn remaining(&self) -> usize {
        match self {
            PacketBody::Empty => 0,
            PacketBody::FromBuf(buf) => buf.remaining(),
            PacketBody::FromHost(buf) => buf.remaining(),
            PacketBody::FromNode(buf) => buf.remaining(),
        }
    }

    fn chunk(&self) -> &[u8] {
        match self {
            PacketBody::Empty => &[],
            PacketBody::FromBuf(buf) => buf.chunk(),
            PacketBody::FromHost(buf) => buf.chunk(),
            PacketBody::FromNode(buf) => buf.chunk(),
        }
    }

    fn advance(&mut self, cnt: usize) {
        match self {
            PacketBody::Empty => {}
            PacketBody::FromBuf(buf) => buf.advance(cnt),
            PacketBody::FromHost(buf) => buf.advance(cnt),
            PacketBody::FromNode(buf) => buf.advance(cnt),
        }
    }
}

impl<'a> Packet<'a> {
    /// decode packet from host side
    pub fn from_host(mut src: kaze_core::Bytes<'a>) -> Result<Self> {
        let hdr = decode_packet(&mut src)?;
        Ok(Self {
            hdr_dirty: false,
            hdr,
            body: PacketBody::FromHost(src),
        })
    }

    /// decode packet from network
    pub fn from_node(hdr: Hdr, src: BufWrapper<BytesMut>) -> Result<Self> {
        Ok(Self {
            hdr_dirty: false,
            hdr,
            body: PacketBody::FromNode(src),
        })
    }

    /// get a response packet for specific error code
    pub fn from_retcode(hdr: Hdr, ret_code: RetCode) -> Result<Self> {
        let hdr = Hdr {
            body_type: String::new(),
            ret_code: ret_code as u32,
            rpc_type: Some(RpcType::Rsp(hdr.seq().unwrap())),
            timeout: 0,
            ..hdr
        };

        Ok(Self {
            hdr_dirty: false,
            hdr,
            body: PacketBody::Empty,
        })
    }

    /// get the header of packet
    pub fn hdr(&self) -> &Hdr {
        &self.hdr
    }

    /// get the mutable header of packet
    pub fn hdr_mut(&mut self) -> &mut Hdr {
        self.hdr_dirty = true;
        &mut self.hdr
    }

    /// get the body size of packet
    pub fn body_len(&self) -> usize {
        self.body.remaining()
    }

    /// get the body of packet
    pub fn body(&mut self) -> impl Buf + use<'_, 'a> {
        &mut self.body
    }

    /// get the mutable body of packet
    pub fn body_mut(&mut self) -> impl BufMut + '_ {
        self.body = PacketBody::FromBuf(BytesMut::new());
        match self.body {
            PacketBody::FromBuf(ref mut data) => data,
            _ => unreachable!(),
        }
    }

    /// get iovec of packet
    pub fn as_iovec(&'a self) -> PacketIoVec<'a> {
        PacketIoVec::new(self)
    }

    /// get buf of packet
    pub fn as_buf(&'a self) -> PacketBuf<'a> {
        PacketBuf::new(self)
    }
}

pub struct PacketIoVec<'a> {
    size_buf: [u8; size_of::<u32>()],
    buf: BytesMut,
    data: PacketIoVecData<'a>,
}

enum PacketIoVecData<'a> {
    FromDirty(&'a Packet<'a>),
    FromHost(kaze_core::Bytes<'a>),
    FromNode(BytesMut),
}

impl<'a> PacketIoVec<'a> {
    pub fn new(packet: &'a Packet<'a>) -> Self {
        let body = if packet.hdr_dirty {
            PacketIoVecData::FromDirty(packet)
        } else {
            match &packet.body {
                PacketBody::FromHost(buf) => {
                    let mut buf = buf.clone();
                    buf.rewind();
                    PacketIoVecData::FromHost(buf)
                }
                PacketBody::FromNode(buf) => {
                    let buf = buf.clone().into_inner();
                    PacketIoVecData::FromNode(buf)
                }
                _ => PacketIoVecData::FromDirty(packet),
            }
        };
        Self {
            size_buf: [0; size_of::<u32>()],
            buf: BytesMut::new(),
            data: body,
        }
    }

    pub fn to_iovec(&'a mut self) -> [IoSlice<'a>; 3] {
        // SAFETY: we ensure that the mutable part will not read when the
        // immutable part is used
        let (mut_self, data_ref) = unsafe { self.split_ref() };
        match data_ref {
            PacketIoVecData::FromDirty(packet) => mut_self.from_dirty(*packet),
            PacketIoVecData::FromHost(buf) => mut_self.from_host(buf),
            PacketIoVecData::FromNode(buf) => mut_self.from_node(buf),
        }
    }

    /// split self into two parts, one is mutable, the other is immutable
    ///
    /// safety: you must ensure that the mutable part will not read when the
    /// immutable part is used.
    unsafe fn split_ref(
        &'a mut self,
    ) -> (&'a mut Self, &'a PacketIoVecData<'a>) {
        let data_ptr = addr_of!(self.data);
        (self, &*data_ptr)
    }

    fn from_dirty(&'a mut self, packet: &'a Packet<'a>) -> [IoSlice<'a>; 3] {
        let buf = &mut self.buf;
        let hdr_size = packet.hdr.encoded_len();
        let total_size =
            size_of::<u32>() * 2 + hdr_size + packet.body.remaining();
        buf.reserve(total_size);
        buf.put_u32_le(total_size as u32);
        buf.put_u32_le(hdr_size as u32);
        packet.hdr.encode(buf).unwrap();
        buf.put(&mut packet.body.clone());
        let mut r = [IoSlice::new(&[]); 3];
        r[0] = IoSlice::new((*buf).as_ref());
        r
    }

    fn from_host(
        &'a mut self,
        buf: &'a kaze_core::Bytes<'a>,
    ) -> [IoSlice<'a>; 3] {
        let total_size = buf.len() as u32;
        self.size_buf.copy_from_slice(&total_size.to_le_bytes());
        let mut r = [IoSlice::new(&[]); 3];
        let (s1, s2) = buf.as_slice();
        r[0] = IoSlice::new(&self.size_buf[..size_of::<u32>()]);
        r[1] = IoSlice::new(s1);
        r[2] = IoSlice::new(s2);
        r
    }

    fn from_node(&'a mut self, buf: &'a BytesMut) -> [IoSlice<'a>; 3] {
        let total_size = buf.len() as u32;
        self.size_buf.copy_from_slice(&total_size.to_le_bytes());
        let mut r = [IoSlice::new(&[]); 3];
        r[0] = IoSlice::new(&self.size_buf[..size_of::<u32>()]);
        r[1] = IoSlice::new(buf.as_ref());
        r
    }
}

pub struct PacketBuf<'a> {
    size_buf: [u8; size_of::<u32>()],
    pos: usize,
    data: PacketBufData<'a>,
}

enum PacketBufData<'a> {
    FromDirty(BytesMut),
    FromHost(kaze_core::Bytes<'a>),
    FromNode(BytesMut),
}

impl Buf for PacketBuf<'_> {
    fn remaining(&self) -> usize {
        (match &self.data {
            PacketBufData::FromDirty(buf) => buf.remaining(),
            PacketBufData::FromHost(buf) => buf.remaining(),
            PacketBufData::FromNode(buf) => buf.remaining(),
        }) + if self.pos < size_of::<u32>() {
            size_of::<u32>() - self.pos
        } else {
            0
        }
    }

    fn chunk(&self) -> &[u8] {
        if self.pos < size_of::<u32>() {
            &self.size_buf[self.pos..]
        } else {
            match &self.data {
                PacketBufData::FromDirty(buf) => buf.chunk(),
                PacketBufData::FromHost(buf) => buf.chunk(),
                PacketBufData::FromNode(buf) => buf.chunk(),
            }
        }
    }

    fn advance(&mut self, mut cnt: usize) {
        let size_rem = size_of::<u32>() - self.pos;
        if size_rem != 0 {
            if size_rem >= cnt {
                self.pos += cnt;
                return;
            }

            // Consume what is left of a
            self.pos = size_of::<u32>();
            cnt -= size_rem;
        }
        match &mut self.data {
            PacketBufData::FromDirty(buf) => buf.advance(cnt),
            PacketBufData::FromHost(buf) => buf.advance(cnt),
            PacketBufData::FromNode(buf) => buf.advance(cnt),
        }
    }
}

impl<'a> PacketBuf<'a> {
    pub fn new(packet: &'a Packet<'a>) -> Self {
        let body = if packet.hdr_dirty {
            PacketBuf::body_from_dirty(packet)
        } else {
            match &packet.body {
                PacketBody::FromHost(buf) => {
                    let mut buf = buf.clone();
                    buf.rewind();
                    PacketBufData::FromHost(buf)
                }
                PacketBody::FromNode(buf) => {
                    let buf = buf.clone().into_inner();
                    PacketBufData::FromNode(buf)
                }
                _ => PacketBuf::body_from_dirty(packet),
            }
        };
        let total_size = match &body {
            PacketBufData::FromDirty(buf) => buf.len(),
            PacketBufData::FromHost(buf) => buf.len(),
            PacketBufData::FromNode(buf) => buf.len(),
        } as u32;
        Self {
            size_buf: total_size.to_le_bytes(),
            pos: 0,
            data: body,
        }
    }

    fn body_from_dirty(packet: &'a Packet<'a>) -> PacketBufData<'a> {
        let mut buf = BytesMut::new();
        buf.put_u32_le(packet.hdr.encoded_len() as u32);
        packet.hdr.encode(&mut buf).unwrap();
        buf.put(&mut packet.body.clone());
        PacketBufData::FromDirty(buf)
    }
}
