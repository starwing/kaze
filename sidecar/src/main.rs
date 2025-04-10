use kaze_sidecar::Options;
use kaze_sidecar::tokio;

fn main() -> anyhow::Result<()> {
    let sidecar = Options::build()?;

    let mut runtime = tokio::runtime::Builder::new_multi_thread();
    if let Some(thread_count) = sidecar.thread_count() {
        runtime.worker_threads(thread_count);
    }
    runtime.enable_all().build()?.block_on(sidecar.run())?;
    Ok(())
}
