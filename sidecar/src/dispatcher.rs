use std::{io, sync::Arc};

use crate::{kaze, socket_pool::SocketPool};

pub struct Dispatcher {
    pool: Arc<SocketPool>,
}

impl Dispatcher {
    pub fn new(pool: Arc<SocketPool>) -> Dispatcher {
        Dispatcher { pool }
    }

    pub fn dispatch(
        &mut self,
        hdr: kaze::Hdr,
        data: kaze_core::Bytes,
    ) -> io::Result<()> {
        println!("transfer_pkg");
        Ok(())
    }
}
