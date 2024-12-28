use std::io::{self, Error, ErrorKind};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use prost::Message;
use tokio_util::codec::{Decoder, Encoder};

use crate::kaze;

/// Network packet codec
///
/// layout:
/// [total_size(4)][hdr_size(4)][hdr][body]
/// total_size = hdr_size + body.len()
///
/// After decode, the buffer will be split to (hdr, data)
/// which the data contains [hdr_size(4)][hdr][body] for forwarding to kaze
/// queue without encode again.
pub struct NetPacketCodec {}

impl Decoder for NetPacketCodec {
    type Item = (kaze::Hdr, BytesMut);
    type Error = Error;

    fn decode(
        &mut self,
        src: &mut BytesMut,
    ) -> Result<Option<Self::Item>, Self::Error> {
        let mut reader = BufAdaptor::new(&src);
        if reader.remaining() < size_of::<u32>() {
            src.reserve(size_of::<u32>() * 2);
            return Ok(None);
        }
        let size = reader.get_u32_le() as usize;
        if size < size_of::<u32>() + reader.remaining() {
            src.reserve(size);
            return Ok(None);
        }
        src.advance(size_of::<u32>());
        let data = src.split_to(size);
        let mut reader = BufAdaptor::new(&data);
        let hdr_size = reader.get_u32_le() as usize;
        if hdr_size > size {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Invalid packet header size",
            ));
        }
        let hdr = kaze::Hdr::decode(reader.take(hdr_size))?;
        Ok(Some((hdr, data)))
    }
}

impl Encoder<(kaze::Hdr, Bytes)> for NetPacketCodec {
    type Error = Error;

    fn encode(
        &mut self,
        item: (kaze::Hdr, Bytes),
        dst: &mut BytesMut,
    ) -> Result<(), Self::Error> {
        let (hdr, data) = item;
        dst.reserve(size_of::<u32>() * 2);
        let hdr_size = hdr.encoded_len();
        dst.put_u32_le((hdr_size + data.remaining()) as u32);
        dst.put_u32_le(hdr_size as u32);
        dst.reserve(hdr_size);
        hdr.encode(dst)?;
        dst.put_slice(data.chunk());
        Ok(())
    }
}

pub struct NetPacketForwardCodec {}

impl Decoder for NetPacketForwardCodec {
    type Item = BytesMut;
    type Error = Error;

    fn decode(
        &mut self,
        src: &mut BytesMut,
    ) -> Result<Option<Self::Item>, Self::Error> {
        let mut reader = BufAdaptor::new(&src);
        if reader.remaining() < size_of::<u32>() * 2 {
            src.reserve(size_of::<u32>() * 2);
            return Ok(None);
        }
        let size = reader.get_u32_le() as usize;
        if size < size_of::<u32>() + reader.remaining() {
            src.reserve(size);
            return Ok(None);
        }
        src.advance(size_of::<u32>());
        let data = src.split_to(size);
        Ok(Some(data))
    }
}

/// Decode a packet
///
/// layouyt:
/// [hdr_size(4)][hdr][body]
pub fn decode_packet<'a>(src: &mut kaze_core::Bytes) -> io::Result<kaze::Hdr> {
    // A KazeBytes should only be used once, it contains only one packet
    assert!(src.pos() == 0);

    if src.remaining() < size_of::<u32>() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "No rooms for packet prefix",
        ));
    }
    let hdr_size = src.get_u32_le() as usize;
    if hdr_size > src.remaining() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Invalid packet header size",
        ));
    }
    let src = src.take(hdr_size);
    Ok(kaze::Hdr::decode(src)?)
}

/// Encode a packet to a KazeBytesMut, KazeBytesMut created from KazeState,
/// should has the same size of header + content
///
/// layouyt:
/// [hdr_size(4)][hdr][body]
#[allow(dead_code)]
pub fn encode_packet(
    src: &mut kaze_core::BytesMut,
    item: (kaze::Hdr, impl Buf),
) -> Result<(), Error> {
    let (hdr, data) = item;
    let hdr_size = hdr.encoded_len();
    if hdr_size > src.remaining_mut() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "No rooms for packet prefix",
        ));
    }
    src.put_u32_le(hdr_size as u32);
    hdr.encode(src)?;
    src.put(data);
    Ok(())
}

struct BufAdaptor<T> {
    inner: T,
    pos: usize,
}

impl<T> BufAdaptor<T> {
    fn new(inner: T) -> BufAdaptor<T> {
        BufAdaptor { inner, pos: 0 }
    }
}

impl<T: AsRef<[u8]>> Buf for BufAdaptor<T> {
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
