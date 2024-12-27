use clap::{crate_version, Parser};
use std::{net::IpAddr, path::PathBuf, sync::LazyLock};

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
    /// Name of the shared memory object
    #[arg(short = 'n', long = "name")]
    pub shmfile: PathBuf,

    /// Identifier for the shared memory object
    #[arg(short, long)]
    pub ident: u32,

    /// location of consul server
    #[arg(short, long)]
    pub consul: Option<http::uri::Uri>,

    /// listen address for the mesh endpoint
    #[arg(short, long, default_value = "0.0.0.0")]
    pub host: IpAddr,

    /// listen port for the mesh endpoint
    #[arg(short, long, default_value = "6081")]
    pub port: u16,

    /// Size of the request (sidecar to host) buffer for shared memory
    #[arg(short = 's', long = "sq", default_value_t = get_page_size())]
    pub sq_bufsize: usize,

    /// Size of the response (host to sidecar) buffer for shared memory
    #[arg(short = 'c', long = "cq", default_value_t = get_page_size())]
    pub cq_bufsize: usize,

    /// Size of the buffer of single individual connection
    #[arg(short = 'p', long = "pack", default_value_t = get_page_size())]
    pub pack_bufsize: usize,

    /// Count of worker threads (0 means autodetect)
    #[arg(short = 'j', long, default_value_t = 0)]
    pub threads: usize,
}

fn get_page_size() -> usize {
    page_size::get()
}
