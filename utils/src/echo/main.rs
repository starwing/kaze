fn main() -> anyhow::Result<()> {
    let mut shm = match kaze_core::KazeState::new("echo", 1, 16, 16) {
        Ok(shm) => shm,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                println!("cleanup previous shm file");
                kaze_core::KazeState::unlink("echo")?;
                kaze_core::KazeState::new("echo", 1, 16, 16)?
            } else {
                println!("Failed to create shared memory: {}", e);
                return Err(e.into());
            }
        }
    };
    loop {
        println!("Waiting for data...");
        let data = shm.pop()?;
        println!("Got data! size={}", data.len());
        let (l, r) = data.as_slices();
        println!(
            "{}{}",
            String::from_utf8_lossy(l),
            String::from_utf8_lossy(r)
        );
    }
}
