mod clap_app;
use std::{
    io::{self, Read, Write},
    mem::MaybeUninit,
    net::TcpStream,
};

use clap::Parser;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = clap_app::Args::parse();
    let mut socket = TcpStream::connect((args.host, args.port))?;

    let mut rl = rustyline::DefaultEditor::new()?;
    if rl.load_history("history.txt").is_err() {
        // No previous history
    }

    let mut reader = socket.try_clone()?;
    let t = std::thread::spawn(move || -> io::Result<()> {
        loop {
            let mut size_buf: MaybeUninit<[u8; size_of::<u32>()]> =
                MaybeUninit::uninit();
            reader.read_exact(unsafe {
                std::slice::from_raw_parts_mut(
                    size_buf.as_mut_ptr() as *mut u8,
                    size_of::<u32>(),
                )
            })?;
            let size =
                u32::from_le_bytes(unsafe { size_buf.assume_init() }) as usize;
            let mut buf = vec![0; size];
            reader.read_exact(&mut buf)?;
            print!("{}", String::from_utf8_lossy(&buf));
        }
    });

    loop {
        let line = match rl.readline("> ") {
            Ok(line) => line,
            Err(rustyline::error::ReadlineError::Eof) => break,
            Err(rustyline::error::ReadlineError::Interrupted) => break,
            Err(e) => {
                writeln!(io::stderr(), "error reading line: {e}")?;
                break;
            }
        };

        let line = line.trim();
        socket.write_all((line.len() as u32).to_le_bytes().as_ref())?;
        socket.write_all(line.as_bytes())?;
        rl.add_history_entry(line)?;
    }

    t.join().expect("thread panicked").map_err(|e| e.into())
}
