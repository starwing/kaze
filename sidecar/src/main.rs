use kaze_sidecar::plugins::{log, prometheus, ratelimit};
use kaze_sidecar::sidecar::Sidecar;
use kaze_sidecar::tracing::error;
use kaze_sidecar::{Shutdown, tokio, tracing::info};

fn main() -> anyhow::Result<()> {
    let sidecar = Sidecar::builder()
        .add::<log::Options>("log")
        .add::<ratelimit::Options>("rate_limit")
        .add::<prometheus::Options>("prometheus")
        .build_pipeline()?
        .build_sidecar()?;

    info!(
        "Starting sidecar wtih threads={}",
        sidecar
            .thread_count()
            .map(|v| v.to_string())
            .unwrap_or("auto".to_string())
    );
    let mut runtime = tokio::runtime::Builder::new_multi_thread();
    if let Some(thread_count) = sidecar.thread_count() {
        runtime.worker_threads(thread_count);
    }

    runtime.enable_all().build()?.block_on(async move {
        let shutdown = Shutdown::default();
        if let Err(err) = sidecar.run_with_shutdown(shutdown).await {
            error!("Sidecar error: {}", err);
        }
    });
    info!("Sidecar stopped");
    Ok(())
}
