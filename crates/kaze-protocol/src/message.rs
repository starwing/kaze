use std::net::SocketAddr;

use crate::packet::Packet;

pub type PacketWithAddr = (Packet, Option<SocketAddr>);

/// response packet, A packet with a destination to send to
#[derive(Debug)]
pub struct Message {
    packet: Packet,
    source: Source,
    destination: Destination,
}

impl<'a> From<PacketWithAddr> for Message {
    fn from((packet, addr): PacketWithAddr) -> Self {
        let source = match addr {
            Some(addr) => Source::from_remote(packet.hdr().src_ident, addr),
            None => Source::from_local(),
        };
        Self {
            packet,
            source,
            destination: Destination::Drop,
        }
    }
}

impl Message {
    /// create a new response
    pub fn new(packet: Packet, src: Source) -> Self {
        Self {
            packet,
            source: src,
            destination: Destination::Drop,
        }
    }

    /// create a new response with a specific destination
    pub fn new_with_destination(
        packet: Packet,
        src: Source,
        dst: Destination,
    ) -> Self {
        Self {
            packet,
            source: src,
            destination: dst,
        }
    }

    /// create a new response from PacketWithAddr and destination
    pub fn from_split(packet: PacketWithAddr, dst: Destination) -> Self {
        let (packet, addr) = packet;
        let src = addr
            .map(|a| Source::from_remote(packet.hdr().src_ident, a))
            .unwrap_or(Source::Host);
        Self {
            packet,
            source: src,
            destination: dst,
        }
    }

    /// get the source
    pub fn source(&self) -> &Source {
        &self.source
    }

    /// convert message into raw packet
    pub fn into_packet(self) -> Packet {
        self.packet
    }

    pub fn split(self) -> (PacketWithAddr, Destination) {
        let addr = match self.source {
            Source::Host => None,
            Source::Node(node) => Some(node.addr),
        };
        ((self.packet, addr), self.destination)
    }

    /// get the packet
    pub fn packet(&self) -> &Packet {
        &self.packet
    }

    /// get the mutable packet
    pub fn packet_mut(&mut self) -> &mut Packet {
        &mut self.packet
    }

    /// set the destination
    pub fn set_destination(&mut self, dst: Destination) {
        self.destination = dst;
    }

    /// get the destination
    pub fn destination(&self) -> &Destination {
        &self.destination
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Source {
    Host,
    Node(Node),
}

impl Source {
    pub fn from_local() -> Self {
        Self::Host
    }

    pub fn from_remote(ident: u32, addr: SocketAddr) -> Self {
        Self::Node(Node::new(ident, addr))
    }

    pub fn ident(&self) -> u32 {
        match self {
            Source::Host => 0,
            Source::Node(node) => node.ident,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Destination {
    Drop,
    Host,
    Node(Node),
    NodeList(Vec<Node>),
}

impl Destination {
    pub fn to_local() -> Self {
        Self::Host
    }

    pub fn to_remote(ident: u32, addr: SocketAddr) -> Self {
        Self::Node(Node::new(ident, addr))
    }

    pub fn to_remote_list(list: impl Iterator<Item = Node>) -> Self {
        Self::NodeList(list.collect())
    }

    pub fn is_drop(&self) -> bool {
        matches!(self, Self::Drop)
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Self::Host)
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Node(_) | Self::NodeList(_))
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Node {
    pub ident: u32,
    pub addr: SocketAddr,
}

impl Node {
    pub const fn new(ident: u32, addr: SocketAddr) -> Self {
        Self { ident, addr }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use crate::proto::Hdr;

    use super::*;

    #[test]
    fn test_message_creation() {
        let addr =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let packet = Packet::from_hdr(Hdr::default());
        let source = Source::from_remote(1, addr);

        let message = Message::new(packet, source);
        assert_eq!(message.source().ident(), 1);
        assert!(matches!(message.destination(), Destination::Drop));

        let message = Message::new_with_destination(
            Packet::from_hdr(Hdr::default()),
            source,
            Destination::to_local(),
        );
        assert!(message.destination().is_local());
    }

    #[test]
    fn test_message_conversion() {
        let addr =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let packet = Packet::from_hdr(Hdr::default());
        let packet_with_addr = (packet, Some(addr));

        let message: Message = packet_with_addr.into();
        let packet = Packet::from_hdr(Hdr::default());
        assert_eq!(message.source().ident(), packet.hdr().src_ident);

        let (pwa, dst) = message.split();
        assert_eq!(pwa.1, Some(addr));
        assert!(dst.is_drop());
    }

    #[test]
    fn test_destination_methods() {
        let addr =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let node = Node::new(1, addr);

        let dst = Destination::to_remote(1, addr);
        assert!(dst.is_remote());
        assert!(!dst.is_local());
        assert!(!dst.is_drop());

        let dst = Destination::to_remote_list(vec![node].into_iter());
        assert!(dst.is_remote());
    }
}
