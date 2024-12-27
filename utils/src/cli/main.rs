use std::{io, panic, thread};

use bytes::BufMut;
use rustyline::error::ReadlineError;

fn main() -> anyhow::Result<()> {
    let mut rl = rustyline::DefaultEditor::new()?;
    println!("before open");
    let mut netside = open_shm("netside")?;
    let mut hostside = open_shm("hostside")?;
    let (ns, nr) = netside.owner();
    let (hs, hr) = hostside.owner();
    assert!(ns == hr);
    assert!(nr == hs);
    if nr != 0 {
        println!("remove previous cli: {}", nr);
    }
    if nr == 0 {
        hostside.set_owner(Some(hostside.pid()), None);
        netside.set_owner(None, Some(netside.pid()));
    }

    println!("after open");
    println!(
        "shm size: (net={}, host={})",
        netside.size(),
        hostside.size()
    );
    println!("shm pid: (net={}, host={})", ns, nr);
    println!("self pid: {}", netside.pid());

    let t = thread::spawn(move || -> anyhow::Result<()> {
        loop {
            let ctx = netside.pop()?;
            let buf = ctx.buffer();
            println!("Got data! size={} data={}", buf.len(), buf);
            ctx.commit();
        }
    });
    loop {
        match rl.readline("> ") {
            Ok(line) => {
                println!("You said: <{}>", line);
                let mut ctx = hostside.push(line.len())?;
                ctx.buffer_mut().put_slice(line.as_bytes());
                ctx.commit(line.len())?;
                println!("after push");
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                drop(hostside);
                break;
            }
            Err(ReadlineError::Eof) => break,
            Err(_) => break,
        }
    }
    // TODO: add shm::close
    // shm.close();
    match t.join() {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(payload) => panic::resume_unwind(payload),
    }
}

fn open_shm(name: &str) -> io::Result<kaze_core::KazeState> {
    kaze_core::KazeState::open(name).or_else(|e| {
        if e.kind() == std::io::ErrorKind::ResourceBusy {
            println!("cleanup previous cli instance");
            kaze_core::KazeState::open(name)
        } else {
            println!("Failed to open shared memory: {}", e);
            Err(e)
        }
    })
}
