use bytes::{BufMut, BytesMut};
use log::error;
use metrics::counter;
use std::sync::Arc;
use std::{collections::HashMap, io, net::SocketAddr};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::Mutex;
use tokio::task::block_in_place;
use tokio::{net::TcpStream, select};
use tokio_stream::StreamExt;
use tokio_util::codec::FramedRead;

use crate::codec::NetPacketForwardCodec;
use crate::dispatcher::Dispatcher;

use crate::codec::NetPacketCodec;

pub struct SocketPool {
    addr_map: Mutex<HashMap<u32, SocketAddr>>,
    sock_map: Mutex<HashMap<SocketAddr, OwnedWriteHalf>>,
    sq: Mutex<kaze_core::KazeState>,
}

impl SocketPool {
    pub fn new(sq: kaze_core::KazeState) -> SocketPool {
        SocketPool {
            addr_map: Mutex::new(HashMap::new()),
            sock_map: Mutex::new(HashMap::new()),
            sq: Mutex::new(sq),
        }
    }

    pub fn split(self) -> (Register, Dispatcher) {
        let arc = Arc::new(self);
        (Register::new(arc.clone()), Dispatcher::new(arc))
    }
}

/// register new incomming connection into socket pool
pub struct Register {
    pool: Arc<SocketPool>,
}

impl Register {
    pub fn new(pool: Arc<SocketPool>) -> Register {
        Register { pool }
    }

    pub async fn incomming(
        &mut self,
        stream: TcpStream,
        addr: SocketAddr,
    ) -> io::Result<()> {
        let (read_half, write_half) = stream.into_split();
        let mut transport = FramedRead::new(read_half, NetPacketCodec {});

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

        // 1. add connection to the map
        self.pool.addr_map.lock().await.insert(hdr.src_ident, addr);
        self.pool.sock_map.lock().await.insert(addr, write_half);

        // 2. transfer the packet
        self.transfer_pkg(data).await?;

        // 3. spawn a new task to handle send to this socket
        let register = Register {
            pool: self.pool.clone(),
        };
        async fn handle_send(
            mut reg: Register,
            transport: FramedRead<OwnedReadHalf, NetPacketCodec>,
        ) -> io::Result<()> {
            let mut transport = FramedRead::new(
                transport.into_inner(),
                NetPacketForwardCodec {},
            );
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
                        return Ok(());
                    }
                };
                reg.transfer_pkg(data).await?;
            }
        }
        tokio::spawn(handle_send(register, transport));
        Ok(())
    }

    async fn transfer_pkg(&mut self, data: BytesMut) -> io::Result<()> {
        let mut sq = self.pool.sq.lock().await;
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
