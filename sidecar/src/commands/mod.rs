mod daemon;
mod send;

pub use daemon::DaemonCommand;
pub use send::SendCommand;

use clap::FromArgMatches;

/// Run a subcommand based on the command name and matches
pub fn run_subcommand(
    cmd: &str,
    matches: &clap::ArgMatches,
) -> anyhow::Result<()> {
    match cmd {
        "daemon" => DaemonCommand::from_arg_matches(matches)?.execute(),
        "send" => SendCommand::from_arg_matches(matches)?.execute(),
        _ => Err(anyhow::anyhow!("Unknown command")),
    }
}
