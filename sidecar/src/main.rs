mod cli;

use clap::CommandFactory as _;
use kaze_sidecar::ConfigMap;
use kaze_sidecar::plugins::{log, prometheus, ratelimit};
use kaze_sidecar::sidecar::Sidecar;
use kaze_sidecar::tracing::error;
use kaze_sidecar::{Shutdown, tokio, tracing::info};

fn main() -> anyhow::Result<()> {
    let mut cmd = cli::Cli::command();
    let ob = Sidecar::options()
        .add::<log::Options>("log")
        .add::<ratelimit::Options>("rate_limit")
        .add::<prometheus::Options>("prometheus");
    for sub in ["run", "dump"] {
        cmd = cmd.mut_subcommand(sub, |c| {
            ob.command()
                .clone()
                .name(sub)
                .about(c.get_about().cloned().unwrap())
                .display_order(0)
        });
    }
    let matches = cmd.clone().get_matches();

    match matches.subcommand() {
        Some(("run", subcmd)) => {
            let sidecar = ob.into_builder(&mut subcmd.clone())?.build()?;
            run_sidecar(sidecar)
        }
        Some(("dump", subcmd)) => {
            let config = ob.into_builder(&mut subcmd.clone())?.config();
            dump_config(config)
        }
        _ => unreachable!("Unknown Subcommand"),
    }
}

fn run_sidecar(sidecar: Sidecar) -> anyhow::Result<()> {
    info!(
        "Starting sidecar wtih threads={}",
        sidecar
            .thread_count()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "auto".to_string())
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

fn dump_config(config: ConfigMap) -> anyhow::Result<()> {
    let mut toml = config.get_toml();
    toml.as_table_mut().sort_values();
    println!("{}", toml.to_string());
    Ok(())
}
