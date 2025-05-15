use std::net::Ipv4Addr;

use clap::Args;
use duration_string::DurationString;

/// Send a message to a given destination
#[derive(Args, Debug, Clone)]
pub struct SendCommand {
    /// The source address to use (EXAMPLE: 0.0.0.1)
    #[arg(short, long, value_name = "ADDR")]
    pub source: Ipv4Addr,

    /// The destination address to send to (EXAMPLE: 0.0.0.1)
    #[arg(short, long, value_name = "ADDR")]
    pub destination: Ipv4Addr,

    /// Timeout to wait for a response
    #[arg(short = 'w', long = "wait", value_name = "TIMEOUT")]
    pub timeout: DurationString,

    /// The message to send
    #[arg(short = 't', long = "type", value_name = "TYPE")]
    pub body_type: String,

    /// the Body is encoded in base64
    #[arg(short = 'b', long = "base64", value_name = "BASE64")]
    pub base64: bool,

    /// the Body is encoded in hex
    #[arg(short = 'x', long = "hex", value_name = "HEX")]
    pub hex: bool,

    /// The message to send
    #[arg(value_name = "BODY")]
    pub body: String,
}

impl SendCommand {
    pub fn execute(&self) -> anyhow::Result<()> {
        // Here you would implement the logic to send the message
        // For now, we just print the command details
        println!(
            "Sending message from {} to {} with timeout {}",
            self.source, self.destination, self.timeout
        );
        println!("Message type: {}", self.body_type);
        println!("Message body: {}", self.body);
        Ok(())
    }
}

#[test]
fn check_command() {
    SendCommand::augment_args(clap::Command::new("test")).debug_assert();
}
