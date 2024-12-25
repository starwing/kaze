use std::io;

fn main() -> anyhow::Result<()> {
    let size = 16;
    let mut netside = make_shm("netside", 1, size)?;
    let mut hostside = make_shm("hostside", 2, size)?;
    hostside.set_owner(None, Some(hostside.pid()));
    netside.set_owner(Some(netside.pid()), None);

    loop {
        println!("Waiting for data...");
        let data = hostside.pop()?;
        let (s1, s2) = data.buffer();
        let len = s1.len() + s2.len();

        let mut ctx = netside.push(len)?;
        let (ms1, ms2) = ctx.buffer_mut();

        split_copy(s1, s2, ms1, ms2);

        println!("Got data! size={}", len);
        println!(
            "{}{}",
            String::from_utf8_lossy(s1),
            String::from_utf8_lossy(s2)
        );

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

fn split_copy(s1: &[u8], s2: &[u8], ms1: &mut [u8], ms2: &mut [u8]) {
    // Ensure the total length of input and output slices matches
    assert_eq!(s1.len() + s2.len(), ms1.len() + ms2.len());

    // Calculate how much of s1 can be copied to ms1
    let first = s1.len().min(ms1.len());
    ms1[..first].copy_from_slice(&s1[..first]);

    // Calculate remain copying
    if s1.len() > ms1.len() {
        let remain = s1.len() - ms1.len();
        ms2[..remain].copy_from_slice(&s1[ms1.len()..]);
        ms2[remain..].copy_from_slice(&s2);
    } else if s1.len() < ms1.len() {
        let remain = ms1.len() - s1.len();
        ms1[s1.len()..].copy_from_slice(&s2[..remain]);
        ms2.copy_from_slice(&s2[remain..]);
    } else {
        ms2.copy_from_slice(s2);
    }
}
