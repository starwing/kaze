use std::io;

use bytes::BufMut;

fn main() -> anyhow::Result<()> {
    let size = 16;
    let mut netside = make_shm("netside", 1, size)?;
    let mut hostside = make_shm("hostside", 2, size)?;
    hostside.set_owner(None, Some(hostside.pid()));
    netside.set_owner(Some(netside.pid()), None);

    loop {
        println!("Waiting for data...");
        let data = hostside.pop()?;
        let buf = data.buffer();
        let len = buf.len();
        println!("Got data! size={}", len);
        println!("data: {}", buf);

        let mut ctx = netside.push(len)?;
        ctx.buffer_mut().put(buf);

        data.commit();
        ctx.commit(len)?;
    }
}

fn make_shm(
    name: &str,
    ident: u32,
    bufsize: usize,
) -> io::Result<kaze_core::KazeState> {
    kaze_core::KazeState::new(name, ident, bufsize).or_else(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            println!("cleanup previous shm file");
            kaze_core::KazeState::unlink(name)?;
            kaze_core::KazeState::new(name, ident, bufsize)
        } else {
            println!("Failed to create shared memory: {}", e);
            Err(e)
        }
    })
}
