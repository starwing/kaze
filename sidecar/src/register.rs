use bytes::{BufMut, BytesMut};
use log::error;
use metrics::counter;
use std::io::{Error, ErrorKind, Result};
use std::sync::Arc;
use std::{collections::HashMap, io, net::SocketAddr};
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::Mutex;
use tokio::task::block_in_place;
use tokio::{net::TcpStream, select};
use tokio_stream::StreamExt;
use tokio_util::codec::FramedRead;

use crate::codec::NetPacketForwardCodec;

use crate::codec::NetPacketCodec;
use crate::resolver::Resolver;

/// register new incomming connection into socket pool
pub struct Register {
    sock_map: Mutex<HashMap<SocketAddr, Arc<Mutex<OwnedWriteHalf>>>>,
    sq: Mutex<kaze_core::KazeState>,
}

impl Register {
    pub fn new(sq: kaze_core::KazeState) -> Self {
        Self {
            sock_map: Mutex::new(HashMap::new()),
            sq: Mutex::new(sq),
        }
    }

    pub async fn incomming(
        self: &Arc<Self>,
        resolver: &Resolver,
        stream: TcpStream,
        addr: SocketAddr,
    ) -> Result<()> {
        let mut transport = FramedRead::new(stream, NetPacketCodec {});

        // 1. waiting for the first packet to read
        let (hdr, data) = select! {
            pkg = transport.next() => if let Some(pkg) = pkg { pkg? }
                else {
                    println!("exit");
                    return Ok(());
                },
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(1000)) => {
                counter!("kaze-read-timeout").increment(1);
                println!("timeout");
                return Ok(());
            }
        };

        // 2. transfer the packet
        self.transfer_pkg(data).await?;

        // 3. add valid connection
        self.add_connection(
            resolver,
            hdr.src_ident,
            transport.into_inner(),
            addr,
        )
        .await;

        Ok(())
    }

    pub async fn find_socket(
        self: &Arc<Self>,
        resolver: &Resolver,
        ident: u32,
    ) -> Result<Arc<Mutex<OwnedWriteHalf>>> {
        if let Some(addr) = resolver.get_node(ident).await {
            if let Some(sock) = self.sock_map.lock().await.get(&addr) {
                Ok(sock.clone())
            } else {
                self.try_connect(resolver, ident, addr).await
            }
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "node not found"))
        }
    }

    async fn try_connect(
        self: &Arc<Self>,
        resolver: &Resolver,
        ident: u32,
        addr: SocketAddr,
    ) -> Result<Arc<Mutex<OwnedWriteHalf>>> {
        let stream = TcpStream::connect(addr).await;
        match stream {
            Ok(stream) => {
                Ok(self.add_connection(resolver, ident, stream, addr).await)
            }
            Err(e) => {
                counter!("kaze-connect-errors").increment(1);
                error!("connect error: {}", e);
                Err(e)
            }
        }
    }

    async fn add_connection(
        self: &Arc<Self>,
        resolver: &Resolver,
        ident: u32,
        conn: TcpStream,
        addr: SocketAddr,
    ) -> Arc<Mutex<OwnedWriteHalf>> {
        let (read_half, write_half) = conn.into_split();
        let write_half = Arc::new(Mutex::new(write_half));

        // 1. add connection to the map
        resolver.add_node(ident, addr).await;
        self.sock_map.lock().await.insert(addr, write_half.clone());

        // 2. spawn a new task to handle send to this socket
        tokio::spawn(self.clone().handle_connection(read_half, addr));
        write_half
    }

    async fn handle_connection(
        self: Arc<Self>,
        read_half: OwnedReadHalf,
        addr: SocketAddr,
    ) -> Result<()> {
        let mut transport =
            FramedRead::new(read_half, NetPacketForwardCodec {});
        loop {
            let data = select! {
                pkg = transport.next() => if let Some(pkg) = pkg { pkg? }
                    else {
                        println!("exit");
                        return Ok(());
                    },
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(1000)) => {
                    counter!("kaze-read-timeout").increment(1);
                    println!("timeout");
                    self.close_connection(transport.into_inner(), addr).await?;
                    return Ok(());
                }
            };
            self.transfer_pkg(data).await?;
        }
    }

    async fn close_connection(
        &self,
        read_half: OwnedReadHalf,
        addr: SocketAddr,
    ) -> Result<()> {
        if let Some(write_half) = self.sock_map.lock().await.remove(&addr) {
            // if the arc's strong count is not 1, just drop it, otherwise, try to
            // shutdown the connection
            if let Ok(write_half) = Arc::try_unwrap(write_half) {
                let mut stream = read_half
                    .reunite(write_half.into_inner())
                    .map_err(|e| Error::new(ErrorKind::Other, e))?;
                stream.shutdown().await?;

                // do not remove from resolver, may reconnect to the same node later
                // self.resolver.remove_node(ident).await;
            }
        }
        Ok(())
    }

    async fn transfer_pkg(&self, data: BytesMut) -> Result<()> {
        let mut sq = self.sq.lock().await;
        let mut ctx = match sq.try_push(data.len()) {
            Ok(ctx) => ctx,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                counter!("kaze-write-blocking").increment(1);
                block_in_place(|| sq.push(data.len())).map_err(|e| {
                    counter!("kaze-write-blocking-errors").increment(1);
                    error!("kaze write blocking error: {}", e);
                    e
                })?
            }
            Err(e) => {
                counter!("kaze-write-errors").increment(1);
                error!("kaze push error: {}", e);
                return Err(e);
            }
        };
        let len = data.len() as usize;
        let mut buf = ctx.buffer_mut();
        buf.put_u32_le(len as u32);
        buf.put(data);
        ctx.commit(len)
    }
}
