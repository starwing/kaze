use std::{
    io::IoSlice,
    ops::{Deref, DerefMut},
    ptr::addr_of,
    sync::Arc,
};

use anyhow::Result;

use lockfree_object_pool::{LinearObjectPool, LinearOwnedReusable};
use prost::Message;
use tokio_util::bytes::{Buf, BufMut, BytesMut};

use crate::{
    codec::{decode_packet, BufWrapper},
    proto::hdr::RpcType,
    proto::Hdr,
    proto::RetCode,
};

/// the bytes pool used by `Packet`
pub type BytesPool = Arc<LinearObjectPool<BufWrapper<BytesMut>>>;

/// create a bytes pool that used by `Packet`
pub fn new_bytes_pool() -> BytesPool {
    Arc::new(LinearObjectPool::new(
        || BufWrapper::new(BytesMut::new()),
        |buf| buf.clear(),
    ))
}

/// the Packet object can be used in anywhere
#[derive(Debug)]
pub struct Packet {
    hdr_dirty: bool,
    hdr: Hdr,
    body: PacketBody,
}

// packet body is empty or wraps a BufWrapper<BytesMut>, the BytesMut holding
// the whole data buffer ([hdr_size(4)][hdr][body]) for packet, and BufWrapper
// pointing the begining of the body bytes.
enum PacketBody {
    Empty,
    FromBuf(BufWrapper<BytesMut>),
    FromHost(LinearOwnedReusable<BufWrapper<BytesMut>>),
    FromNode(BufWrapper<BytesMut>),
}

impl std::fmt::Debug for PacketBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "Empty"),
            Self::FromBuf(arg0) => {
                f.debug_tuple("FromBuf").field(arg0).finish()
            }
            Self::FromHost(arg0) => {
                f.debug_tuple("FromHost").field(Deref::deref(arg0)).finish()
            }
            Self::FromNode(arg0) => {
                f.debug_tuple("FromNode").field(arg0).finish()
            }
        }
    }
}

impl Packet {
    /// decode packet from host side
    ///
    /// src contains the whole data buffer [hdr_size(4)][hdr][body]
    pub fn from_host(
        mut src: LinearOwnedReusable<BufWrapper<BytesMut>>,
    ) -> Result<Self> {
        src.rewind();
        let hdr = decode_packet(DerefMut::deref_mut(&mut src))?;
        Ok(Self {
            hdr_dirty: false,
            hdr,
            body: PacketBody::FromHost(src),
        })
    }

    /// decode packet from network
    ///
    /// src must be a BufWrapper<BytesMut> which the BytesMut contains the whole
    /// data buffer ([hdr_size(4)][hdr][body]), and the cursor of BufWrapper
    /// must be moved to the beginning of the body.
    pub(crate) fn from_node(
        hdr: Hdr,
        src: BufWrapper<BytesMut>,
    ) -> Result<Self> {
        Ok(Self {
            hdr_dirty: false,
            hdr,
            body: PacketBody::FromNode(src),
        })
    }

    /// create a packet from header
    pub fn from_hdr(hdr: Hdr) -> Self {
        Self {
            hdr_dirty: false,
            hdr,
            body: PacketBody::Empty,
        }
    }

    /// get a response packet for specific error code
    pub fn from_retcode(hdr: Hdr, ret_code: RetCode) -> Self {
        Self::from_hdr(Hdr {
            body_type: String::new(),
            ret_code: ret_code as u32,
            rpc_type: Some(RpcType::Rsp(hdr.seq().unwrap_or(0))),
            timeout: 0,
            ..hdr
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
        self.body().remaining()
    }

    /// get the body of packet
    pub fn body(&self) -> &[u8] {
        match &self.body {
            PacketBody::Empty => &[],
            PacketBody::FromBuf(ref data) => data.as_ref(),
            PacketBody::FromHost(ref data) => data.as_ref(),
            PacketBody::FromNode(ref data) => data.as_ref(),
        }
    }

    /// get the mutable body of packet
    pub fn body_mut(&mut self) -> impl BufMut + '_ {
        self.body = PacketBody::FromBuf(BufWrapper::new(BytesMut::new()));
        match self.body {
            PacketBody::FromBuf(ref mut data) => data.as_inner_mut(),
            _ => unreachable!(),
        }
    }

    /// get iovec of packet, format: [total_size(4)][hdr_size(4)][hdr][body
    pub fn as_iovec(
        &self,
        pool: &Arc<LinearObjectPool<BufWrapper<BytesMut>>>,
    ) -> PacketIoVec {
        PacketIoVec::new(self, pool)
    }

    /// get buf of packet, format: [hdr_size(4)][hdr][body
    pub fn as_buf(
        &self,
        pool: &Arc<LinearObjectPool<BufWrapper<BytesMut>>>,
    ) -> PacketBuf {
        PacketBuf::new(self, pool)
    }
}

pub struct PacketIoVec<'a> {
    size_buf: [u8; size_of::<u32>()],
    buf: LinearOwnedReusable<BufWrapper<BytesMut>>,
    data: PacketIoVecData<'a>,
}

enum PacketIoVecData<'a> {
    FromDirty(&'a Packet),
    FromWhole(&'a BytesMut),
}

impl<'a> PacketIoVec<'a> {
    pub fn new(
        packet: &'a Packet,
        pool: &Arc<LinearObjectPool<BufWrapper<BytesMut>>>,
    ) -> Self {
        Self {
            size_buf: [0; size_of::<u32>()],
            buf: pool.pull_owned(),
            data: if packet.hdr_dirty {
                PacketIoVecData::FromDirty(packet)
            } else {
                match &packet.body {
                    PacketBody::FromHost(buf) => {
                        PacketIoVecData::FromWhole(buf.as_inner())
                    }
                    PacketBody::FromNode(buf) => {
                        PacketIoVecData::FromWhole(buf.as_inner())
                    }
                    _ => PacketIoVecData::FromDirty(packet),
                }
            },
        }
    }

    pub fn to_iovec(&'a mut self) -> [IoSlice<'a>; 2] {
        // SAFETY: we ensure that the mutable part will not read when the
        // immutable part is used
        let (mut_self, data_ref) = unsafe { self.split_ref() };
        match data_ref {
            PacketIoVecData::FromDirty(packet) => {
                mut_self.from_hdr_body(*packet)
            }
            PacketIoVecData::FromWhole(buf) => mut_self.from_whole(*buf),
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

    fn from_hdr_body(&'a mut self, packet: &'a Packet) -> [IoSlice<'a>; 2] {
        let buf = self.buf.as_inner_mut();
        let hdr_size = packet.hdr.encoded_len();
        let body = packet.body();
        let total_size = size_of::<u32>() + hdr_size + body.remaining();
        buf.reserve(total_size);
        buf.put_u32_le(total_size as u32);
        buf.put_u32_le(hdr_size as u32);
        packet.hdr.encode(buf).unwrap();
        [IoSlice::new((*buf).as_ref()), IoSlice::new(body)]
    }

    fn from_whole(&'a mut self, buf: &'a BytesMut) -> [IoSlice<'a>; 2] {
        let total_size = buf.len() as u32;
        self.size_buf.copy_from_slice(&total_size.to_le_bytes());
        [IoSlice::new(&self.size_buf), IoSlice::new(buf.as_ref())]
    }
}

pub enum PacketBuf<'a> {
    FromHdrBody {
        hdr: LinearOwnedReusable<BufWrapper<BytesMut>>,
        body: &'a [u8],
    },
    FromWhole(BufWrapper<&'a BytesMut>),
}

impl Buf for PacketBuf<'_> {
    fn remaining(&self) -> usize {
        match self {
            PacketBuf::FromHdrBody { hdr, body, .. } => {
                hdr.remaining() + body.remaining()
            }
            PacketBuf::FromWhole(buf) => buf.remaining(),
        }
    }

    fn chunk(&self) -> &[u8] {
        match self {
            PacketBuf::FromHdrBody { hdr, body } => {
                if hdr.chunk().len() > 0 {
                    hdr.chunk()
                } else {
                    body
                }
            }
            PacketBuf::FromWhole(buf) => buf.chunk(),
        }
    }

    fn advance(&mut self, cnt: usize) {
        match self {
            PacketBuf::FromHdrBody { hdr, body } => {
                let first = cnt.min(hdr.remaining());
                hdr.advance(first);
                body.advance(cnt - first);
            }
            PacketBuf::FromWhole(buf) => buf.advance(cnt),
        }
    }
}

impl<'a> PacketBuf<'a> {
    pub fn new(
        packet: &'a Packet,
        pool: &Arc<LinearObjectPool<BufWrapper<BytesMut>>>,
    ) -> Self {
        if packet.hdr_dirty {
            PacketBuf::from_hdr_body(packet, pool.pull_owned())
        } else {
            match &packet.body {
                PacketBody::FromHost(buf) => {
                    PacketBuf::FromWhole(BufWrapper::new(buf.as_inner()))
                }
                PacketBody::FromNode(buf) => {
                    PacketBuf::FromWhole(BufWrapper::new(buf.as_inner()))
                }
                _ => PacketBuf::from_hdr_body(packet, pool.pull_owned()),
            }
        }
    }

    fn from_hdr_body(
        packet: &'a Packet,
        mut buf: LinearOwnedReusable<BufWrapper<BytesMut>>,
    ) -> Self {
        let mut bytes = buf.as_inner_mut();
        bytes.reserve(size_of::<u32>() + packet.hdr.encoded_len());
        bytes.put_u32_le(packet.hdr.encoded_len() as u32);
        packet.hdr.encode(&mut bytes).unwrap();
        PacketBuf::FromHdrBody {
            hdr: buf,
            body: packet.body(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::codec::NetPacketCodec;

    use super::*;

    use futures::{executor::block_on, StreamExt};
    use tokio_util::{bytes::BytesMut, codec::FramedRead};

    async fn test_packet(packet: &Packet, pool: BytesPool) -> Packet {
        let buf = packet.as_buf(&pool);

        let mut bytes = pool.pull_owned();
        let bytes = bytes.as_inner_mut();
        bytes.put_u32_le(buf.remaining() as u32);
        bytes.put(buf);

        let cursor = Cursor::new(bytes);
        let codec = NetPacketCodec::new(new_bytes_pool());
        let mut framed = FramedRead::new(cursor, codec);
        framed.next().await.unwrap().unwrap()
    }

    #[test]
    fn test_packet_body_mut() {
        let mut packet = Packet::from_hdr(Hdr {
            body_type: "test_data".into(),
            ..Hdr::default()
        });
        packet.body_mut().put_slice(b"test data");

        let out = block_on(test_packet(&packet, new_bytes_pool()));
        assert_eq!(out.body(), b"test data");
    }

    #[test]
    fn test_packet_as_buf_from_host() {
        let pool = new_bytes_pool();
        let mut packet = Packet::from_hdr(Hdr::default());
        packet.body_mut().put_slice(b"test data");
        let mut host_data = pool.pull_owned();
        host_data.as_inner_mut().put(packet.as_buf(&pool));

        let packet = Packet::from_host(host_data).unwrap();
        let out = block_on(test_packet(&packet, pool));

        assert_eq!(out.hdr(), packet.hdr());
        assert_eq!(out.body(), packet.body());
    }

    #[test]
    fn test_packet_as_buf_from_node() {
        let pool = new_bytes_pool();
        let mut packet = Packet::from_hdr(Hdr::default());
        packet.body_mut().put_slice(b"test data");
        let mut bytes = BufWrapper::new(BytesMut::new());
        bytes.as_inner_mut().put(packet.as_buf(&pool));
        let hdr = decode_packet(&mut bytes).unwrap();

        let packet = Packet::from_node(hdr, bytes).unwrap();
        let out = block_on(test_packet(&packet, pool));

        assert_eq!(out.hdr(), packet.hdr());
        assert_eq!(out.body(), packet.body());
    }

    #[test]
    fn test_packet_as_iovec() {
        let pool = new_bytes_pool();

        // create iovec
        let hdr = Hdr::default();
        let mut packet = Packet::from_hdr(hdr);
        packet.body_mut().put_slice(b"test data");
        let mut iovec = packet.as_iovec(&pool);
        let iovec = iovec.to_iovec();

        // make it readable
        let mut bytes = pool.pull_owned();
        let bytes = bytes.as_inner_mut();
        bytes.put_slice(iovec[0].as_ref());
        bytes.put_slice(iovec[1].as_ref());

        let cursor = std::io::Cursor::new(bytes);

        let codec = NetPacketCodec::new(new_bytes_pool());
        let mut framed = FramedRead::new(cursor, codec);
        let out = block_on(framed.next()).unwrap().unwrap();

        assert_eq!(packet.hdr, out.hdr);
    }
}
