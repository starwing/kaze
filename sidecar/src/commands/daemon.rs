use std::net::Ipv4Addr;

use clap::{Args, Subcommand};

/// Daemon command for managing the Sidecar daemon.
#[derive(Args, Clone, Debug)]
pub struct DaemonCommand {
    #[command(subcommand)]
    pub subcmd: SubCommand,
}

impl DaemonCommand {
    /// Execute the command
    pub fn execute(&self) -> anyhow::Result<()> {
        match &self.subcmd {
            SubCommand::Start => {
                // Logic to start the daemon
                println!("Starting the Sidecar daemon...");
            }
            SubCommand::Stop => {
                // Logic to stop the daemon
                println!("Stopping the Sidecar daemon...");
            }
            SubCommand::Restart => {
                // Logic to restart the daemon
                println!("Restarting the Sidecar daemon...");
            }
            SubCommand::Status => {
                // Logic to check the status of the daemon
                println!("Checking the status of the Sidecar daemon...");
            }
            SubCommand::Query(query) => {
                // Logic to query the daemon
                println!("Querying the Sidecar daemon at address: {}", query.address);
            }
        }
        Ok(())
    }
}

#[derive(Subcommand, Clone, Debug)]
pub enum SubCommand {
    /// Start the Sidecar daemon.
    Start,

    /// Stop the Sidecar daemon.
    Stop,

    /// Restart the Sidecar daemon.
    Restart,

    /// Status of the Sidecar daemon.
    Status,

    /// Query the Sidecar daemon.
    Query(QueryCommand),
}

#[derive(Args, Clone, Debug)]
pub struct QueryCommand {
    /// The address to query
    #[arg(value_name = "QUERY")]
    pub address: Ipv4Addr,
}
