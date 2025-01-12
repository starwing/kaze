mod options;

use std::{borrow::Cow, net::Ipv4Addr, ptr::addr_of_mut, sync::Arc};

use anyhow::{anyhow, bail, Context, Result};
use bytes::{Buf, BufMut};
use metrics::counter;
use tokio::{sync::Mutex, task::block_in_place};
use tracing::{error, info, warn};

use kaze_core::{self, KazeState};

pub use options::Options;

pub struct Edge {
    cq: KazeState, // completion queue
    sq: KazeState, // submission queue
}

impl std::fmt::Display for Edge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Edge")
            .field("sq", &self.sq.name())
            .field("cq", &self.cq.name())
            .finish()
    }
}

impl Edge {
    pub fn cq_name(&self) -> Cow<'_, str> {
        self.cq.name()
    }
    pub fn sq_name(&self) -> Cow<'_, str> {
        self.sq.name()
    }
    pub fn split(self) -> (Receiver, Sender) {
        (Receiver::new(self.cq), Sender::new(self.sq))
    }

    fn new_kaze_pair(
        prefix: impl AsRef<str>,
        ident: Ipv4Addr,
        sq_bufsize: usize,
        cq_bufsize: usize,
        unlink: bool,
    ) -> Result<Self> {
        let (sq_name, cq_name) = Self::get_kaze_pair_names(prefix, ident);

        if KazeState::exists(&sq_name).context("Failed to check shm queue")? {
            if !unlink {
                let sq = KazeState::open(&sq_name)
                    .context("Failed to open submission queue")?;
                let (sender, receiver) = sq.owner();
                bail!(
                "shm queue {} already exists, previous kaze sender={} receiver={}",
                sq_name,
                sender,
                receiver
            );
            } else {
                if let Err(e) = KazeState::unlink(&sq_name) {
                    warn!(error = %e, "Failed to unlink submission queue");
                }
                if let Err(e) = KazeState::unlink(&cq_name) {
                    warn!(error = %e, "Failed to unlink completion queue");
                }
            }
        }

        let ident = ident.to_bits();
        let page_size = page_size::get();
        let sq_bufsize = KazeState::aligned_bufsize(sq_bufsize, page_size);
        let cq_bufsize = KazeState::aligned_bufsize(cq_bufsize, page_size);
        let mut sq = KazeState::new(&sq_name, ident, sq_bufsize)
            .context("Failed to create submission queue")?;
        let mut cq = KazeState::new(&cq_name, ident, cq_bufsize)
            .context("Failed to create completion queue")?;
        sq.set_owner(Some(sq.pid()), None);
        cq.set_owner(None, Some(cq.pid()));
        Ok(Self { cq, sq })
    }

    pub fn unlink(prefix: impl AsRef<str>, ident: Ipv4Addr) -> Result<()> {
        let (sq_name, cq_name) = Self::get_kaze_pair_names(prefix, ident);
        info!("unlink submission queue: {}", sq_name);
        KazeState::unlink(&sq_name)
            .context("Failed to unlink submission queue")?;
        info!("unlink completion queue: {}", cq_name);
        KazeState::unlink(&cq_name)
            .context("Failed to unlink completion queue")?;
        Ok(())
    }

    fn get_kaze_pair_names(
        prefix: impl AsRef<str>,
        ident: Ipv4Addr,
    ) -> (String, String) {
        let addr = ident.to_string();
        let sq_name = format!("{}_sq_{}", prefix.as_ref(), addr);
        let cq_name = format!("{}_cq_{}", prefix.as_ref(), addr);
        (sq_name, cq_name)
    }
}

pub struct Receiver {
    cq: KazeState,
}

impl Receiver {
    fn new(cq: KazeState) -> Self {
        Self { cq }
    }

    pub fn lock(&self) -> kaze_core::Guard {
        self.cq.lock()
    }

    pub async fn recv<'a>(
        &'a mut self,
    ) -> Result<Option<kaze_core::PopContext<'a>>> {
        let raw_self = addr_of_mut!(*self);
        // SAFETY: the WouldBlock branch is not borrow self, but the borrow checker
        // cannot detect it. Use `addr_of_mut!` to bypass the borrow checker.
        //
        // See also:
        // https://stackoverflow.com/questions/58295535
        match unsafe { (*raw_self).cq.try_pop() } {
            Ok(ctx) => return Ok(Some(ctx)),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                counter!("kaze_pop_blocking_total").increment(1);
                self.block_recv()
            }
            Err(e) => {
                counter!("kaze_pop_errors_total").increment(1);
                error!(error = %e, "Error reading from kaze");
                return Err(e.into());
            }
        }
    }

    fn block_recv(&mut self) -> Result<Option<kaze_core::PopContext<'_>>> {
        match block_in_place(|| self.cq.pop()) {
            Ok(ctx) => Ok(Some(ctx)),
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
                info!("completion queue closed");
                return Ok(None);
            }
            Err(e) => {
                counter!("kaze_pop_blocking_errors_total").increment(1);
                error!(error = %e, "Error reading from blocking kaze");
                return Err(e.into());
            }
        }
    }
}

pub struct Sender {
    sq: Arc<Mutex<KazeState>>,
}

impl Clone for Sender {
    fn clone(&self) -> Self {
        Self {
            sq: self.sq.clone(),
        }
    }
}

impl Sender {
    fn new(sq: KazeState) -> Self {
        Self {
            sq: Arc::new(Mutex::new(sq)),
        }
    }

    pub async fn ident(&self) -> u32 {
        self.sq.lock().await.ident()
    }

    pub async fn lock(&self) -> kaze_core::Guard {
        self.sq.lock().await.lock()
    }

    pub async fn send(&self, buf: impl Buf) -> Result<()> {
        let mut sq = self.sq.lock().await;
        let mut ctx = match sq.try_push(buf.remaining()) {
            Ok(ctx) => ctx,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                counter!("kaze_push_blocking_total").increment(1);
                block_in_place(|| sq.push(buf.remaining())).map_err(|e| {
                    counter!("kaze_push_blocking_errors_total").increment(1);
                    anyhow!("kaze blocking push error: {}", e)
                })?
            }
            Err(e) => {
                counter!("kaze_push_errors_total").increment(1);
                bail!("kaze push error: {}", e);
            }
        };
        let len = buf.remaining() as usize;
        let mut dst = ctx.buffer_mut();
        dst.put_u32_le(len as u32);
        dst.put(buf);
        ctx.commit(len)?;
        counter!("kaze_submission_packets_total").increment(1);
        Ok(())
    }
}
