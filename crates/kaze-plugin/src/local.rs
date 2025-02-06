use std::{net::{IpAddr, Ipv4Addr, SocketAddr}, sync::Mutex};

use kaze_protocol::message::Node;

static LOCAL_NODE: Mutex<Node> = Mutex::new(Node::new(
    0,
    SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
));

/// set local node ident and address
pub fn set_local_node(ident: u32, addr: SocketAddr) {
    let mut node = LOCAL_NODE.lock().unwrap();
    node.ident = ident;
    node.addr = addr;
}

/// get local node ident and address
pub fn local_node() -> Node {
    LOCAL_NODE.lock().unwrap().clone()
}
