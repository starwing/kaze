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
