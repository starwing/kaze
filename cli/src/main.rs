use clap::{Args, Command};
use clap_app::Config;

mod clap_app;

fn main() -> anyhow::Result<()> {
    let cli = Command::new("foo");
    let cli = Config::augment_args(cli);
    let matches = cli.get_matches();
    let port: Option<&u16> = matches.get_one("port");
    println!("{:?}", port);
    Ok(())
}
