use clap::{crate_version, ColorChoice, Parser};
use std::{io::IsTerminal, net::IpAddr, path::PathBuf, sync::LazyLock};

static VERSION: LazyLock<String> = LazyLock::new(|| {
    let git_version = bugreport::git_version!(fallback = "");

    if git_version.is_empty() {
        crate_version!().to_string()
    } else {
        format!("{} ({})", crate_version!(), git_version)
    }
});

#[derive(Parser)]
#[command(version = VERSION.as_str(), about)]
pub struct Args {
    /// colorize log output
    #[arg(default_value = is_interactive_output())]
    pub color: ColorChoice,

    /// Name of the shared memory object
    #[arg(short, long)]
    pub shm_name: PathBuf,

    /// Identifier for the shared memory object
    #[arg(short, long)]
    pub ident: u32,

    /// location of consul server
    #[arg(short, long)]
    pub consul: http::uri::Uri,

    /// listen address for the mesh endpoint
    #[arg(short, long)]
    pub host: IpAddr,

    /// listen port for the mesh endpoint
    #[arg(short, long, default_value = "6081")]
    pub port: u16,

    /// Size of the local socket buffer size
    #[arg(short, long, default_value_t = get_page_size())]
    pub bufsize: usize,

    /// Size of the networking (outcomming) buffer for shared memory
    #[arg(short, long, default_value_t = get_page_size())]
    pub net_bufsize: usize,

    /// Size of the host (incomming) buffer for shared memory
    #[arg(short, long, default_value_t = get_page_size())]
    pub host_bufsize: usize,
}

fn get_page_size() -> usize {
    page_size::get()
}

fn is_interactive_output() -> &'static str {
    if std::io::stdout().is_terminal() {
        "auto"
    } else {
        "never"
    }
}
