use std::{panic, thread};

use rustyline::error::ReadlineError;

fn main() -> anyhow::Result<()> {
    let mut rl = rustyline::DefaultEditor::new()?;
    println!("before open");
    let mut shm = match kaze_core::KazeState::open("echo") {
        Ok(shm) => shm,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::ResourceBusy {
                println!("cleanup previous cli instance");
                kaze_core::KazeState::cleanup_host("echo")?;
                kaze_core::KazeState::open("echo")?
            } else {
                println!("Failed to open shared memory: {}", e);
                return Err(e.into());
            }
        }
    };
    println!("after open");

    let mut receiver = unsafe { shm.dup() };
    let t = thread::spawn(move || -> anyhow::Result<()> {
        loop {
            let data = receiver.pop()?;
            let (l, r) = data.as_slices();
            println!(
                "{}{}",
                String::from_utf8_lossy(l),
                String::from_utf8_lossy(r)
            );
        }
    });
    loop {
        match rl.readline("> ") {
            Ok(line) => {
                println!("You said: <{}>", line);
                shm.push(line.as_bytes())?;
                println!("after push");
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
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
