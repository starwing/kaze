use clap::{Args, Parser, Subcommand};

use crate::commands::{DaemonCommand, SendCommand};

#[derive(Parser, Debug)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Tools,
}

#[derive(Subcommand, Clone, Debug)]
pub enum Tools {
    /// Run the sidecar with the given config and host command
    Run(DummyCommand),
    /// Dump the config from config file and command line flags
    Dump(DummyCommand),
    /// Send a message to a given destination
    Send(SendCommand),
    /// Daemon command for managing the Sidecar daemon
    Daemon(DaemonCommand),
}

#[derive(Args, Clone, Debug)]
#[command(disable_help_flag = true)]
pub struct DummyCommand {
    #[arg(value_name = "ARGS")]
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
}
