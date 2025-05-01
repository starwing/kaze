use std::{
    fmt::Debug,
    io::{Error, ErrorKind},
};

use anyhow::{Context, Result, bail};
use prost::Message;
use tokio_util::{
    bytes::{Buf, BufMut, BytesMut},
    codec::{Decoder, Encoder},
};

use crate::{
    packet::{BytesPool, Packet},
    proto,
};

/// Network packet codec
///
/// layout:
/// [total_size(4)][hdr_size(4)][hdr][body]
/// total_size = hdr_size + body.len()
///
/// After decode, the buffer will be split to (hdr, data)
/// which the data contains [hdr_size(4)][hdr][body] for forwarding to proto
/// queue without encode again.
#[derive(Clone, Copy)]
pub struct NetPacketDecoder {}

impl<'a> NetPacketDecoder {
    pub fn new() -> Self {
        NetPacketDecoder {}
    }
}

impl<'a> Decoder for NetPacketDecoder {
    type Item = Packet;
    type Error = anyhow::Error;

    #[tracing::instrument(skip(self, src))]
    fn decode(
        &mut self,
        src: &mut BytesMut,
    ) -> Result<Option<Self::Item>, Self::Error> {
        let mut reader = BufWrapper::new(&src);
        if reader.remaining() < size_of::<u32>() {
            src.reserve(size_of::<u32>() * 2);
            return Ok(None);
        }
        let size = reader.get_u32_le() as usize;
        if size_of::<u32>() + reader.remaining() < size {
            src.reserve(size);
            return Ok(None);
        }
        src.advance(size_of::<u32>());
        let data = src.split_to(size);
        let mut reader = BufWrapper::new(&data);
        let hdr_size = reader.get_u32_le() as usize;
        if hdr_size > size {
            bail!("Invalid packet header size={} hdr_size={}", size, hdr_size);
        }
        let hdr = proto::Hdr::decode(reader.take(hdr_size))
            .context("Failed to decode packet")?;
        let mut data = BufWrapper::new(data);
        data.advance(hdr_size + size_of::<u32>());
        Ok(Some(Packet::from_node(hdr, data)?))
    }
}

#[derive(Clone)]
pub struct NetPacketCodec {
    decoder: NetPacketDecoder,
    pool: BytesPool,
}

impl Debug for NetPacketCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetPacketCodec").finish()
    }
}

impl NetPacketCodec {
    pub fn new(pool: BytesPool) -> Self {
        NetPacketCodec {
            decoder: NetPacketDecoder::new(),
            pool,
        }
    }

    pub fn pool(&self) -> &BytesPool {
        &self.pool
    }
}

impl Decoder for NetPacketCodec {
    type Item = Packet;
    type Error = anyhow::Error;

    #[tracing::instrument(skip(self, src))]
    fn decode(
        &mut self,
        src: &mut BytesMut,
    ) -> std::result::Result<Option<Self::Item>, Self::Error> {
        self.decoder.decode(src)
    }
}

impl Encoder<Packet> for NetPacketCodec {
    type Error = anyhow::Error;

    #[tracing::instrument(skip(self, dst))]
    fn encode(
        &mut self,
        item: Packet,
        dst: &mut BytesMut,
    ) -> Result<(), Self::Error> {
        let mut buf = item.as_buf(&self.pool);
        dst.reserve(size_of::<u32>() + buf.remaining());
        dst.put_u32_le(buf.remaining() as u32);
        dst.put(&mut buf);
        Ok(())
    }
}

/// Decode a packet
///
/// layouyt:
/// `[hdr_size(4)][hdr][body]`
///
/// the hdr will decoded and returned, remaining data in src is the body of
/// packet.
#[tracing::instrument(skip(src))]
pub fn decode_packet<'a>(mut src: impl Buf) -> Result<proto::Hdr> {
    if src.remaining() < size_of::<u32>() {
        bail!("No rooms for packet prefix, remaining={}", src.remaining());
    }
    let hdr_size = src.get_u32_le() as usize;
    if hdr_size > src.remaining() {
        bail!(
            "Invalid packet header size={}, remaining={}",
            hdr_size,
            src.remaining()
        );
    }
    let buf = src.take(hdr_size);
    Ok(proto::Hdr::decode(buf).context("Failed to decode Hdr")?)
}

/// Encode a packet to a KazeBytesMut, KazeBytesMut created from KazeState,
/// should has the same size of header + content
///
/// layouyt:
/// [hdr_size(4)][hdr][body]
pub fn encode_packet(
    mut src: impl BufMut,
    hdr: &proto::Hdr,
    data: impl Buf,
) -> Result<(), Error> {
    let hdr_size = hdr.encoded_len();
    if hdr_size > src.remaining_mut() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "No rooms for packet prefix",
        ));
    }
    src.put_u32_le(hdr_size as u32);
    hdr.encode(&mut src)?;
    src.put(data);
    Ok(())
}

/// A wrapper for other buffer. giving a pos that can rewind.
///
/// It can be used to indicate the decoded position of a buffer, and If needed,
/// rewind to its beginning to retrieve the original buffer.
#[derive(Debug, Clone)]
pub struct BufWrapper<T> {
    inner: T,
    pos: usize,
}

impl<T> BufWrapper<T> {
    /// Create a new BufWrapper
    pub fn new(inner: T) -> BufWrapper<T> {
        BufWrapper { inner, pos: 0 }
    }

    /// Rewind the buffer
    pub fn rewind(&mut self) {
        self.pos = 0
    }

    /// Get the inner buffer
    pub fn as_inner(&self) -> &T {
        &self.inner
    }

    /// Get the inner buffer mut
    pub fn as_inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Convert the inner buffer
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl BufWrapper<BytesMut> {
    /// Clear the buffer
    pub fn clear(&mut self) {
        self.pos = 0;
        self.inner.clear();
    }
}

impl<T: AsRef<[u8]>> AsRef<[u8]> for BufWrapper<T> {
    fn as_ref(&self) -> &[u8] {
        &self.inner.as_ref()[self.pos..]
    }
}

impl<T: AsRef<[u8]>> Buf for BufWrapper<T> {
    fn remaining(&self) -> usize {
        self.inner.as_ref().len() - self.pos
    }

    fn chunk(&self) -> &[u8] {
        &self.inner.as_ref()[self.pos..]
    }

    fn advance(&mut self, cnt: usize) {
        self.pos += cnt;
    }
}
