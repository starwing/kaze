use clap::{crate_version, Parser};
use clap_serde_derive::ClapSerde;
use log::info;
use serde::Deserialize;
use std::{
    io::{Error, ErrorKind, Result},
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    str::FromStr,
    sync::LazyLock,
};

type OptArgs = <Args as ClapSerde>::Opt;

pub fn parse_args() -> Result<Args> {
    let mut args = Args::default();
    let opt_args = OptArgs::parse();

    let default_config_file = PathBuf::from_str("sidecar.toml").unwrap();
    let config_file = opt_args
        .config
        .as_ref()
        .or(Some(&default_config_file))
        .filter(|p| p.exists());

    if let Some(config) = config_file {
        info!("use config file {}", config.display());
        let file_args: OptArgs =
            toml::from_str(&std::fs::read_to_string(config)?)
                .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        args = args.merge(file_args);
    }

    args = args.merge(opt_args);

    args.validate()?;
    Ok(args)
}

static VERSION: LazyLock<String> = LazyLock::new(|| {
    let git_version = bugreport::git_version!(fallback = "");

    if git_version.is_empty() {
        crate_version!().to_string()
    } else {
        format!("{} ({})", crate_version!(), git_version)
    }
});

#[derive(ClapSerde, Parser, Deserialize, Debug)]
#[command(version = VERSION.as_str(), about)]
pub struct Args {
    /// Name of config file (default: sidecar.toml)
    #[arg(short = 'f', long = "config")]
    #[default(PathBuf::new())]
    pub config: PathBuf,

    /// Name of the shared memory object
    #[arg(short = 'n', long = "name")]
    pub shmfile: PathBuf,

    /// Identifier for the shared memory object
    #[arg(short, long)]
    #[default(Ipv4Addr::new(0, 0, 0, 0))]
    pub ident: Ipv4Addr,

    /// listen address for the mesh endpoint
    #[arg(short, long)]
    #[default(":6081".to_owned())]
    pub listen: String,

    /// location of consul server
    #[arg(short = 'r', long = "resolve")]
    #[default("".to_owned())]
    // not support for uri now
    // #[serde(with = "http_serde::uri")]
    // pub consul: http::uri::Uri,
    pub consul: String,

    /// Size of the request (sidecar to host) buffer for shared memory
    #[arg(short = 's', long = "sq")]
    #[default(get_page_size())]
    pub sq_bufsize: usize,

    /// Size of the response (host to sidecar) buffer for shared memory
    #[arg(short = 'c', long = "cq")]
    #[default(get_page_size())]
    pub cq_bufsize: usize,

    /// Count of worker threads (0 means autodetect)
    #[arg(short = 'j', long)]
    #[default(0)]
    pub threads: usize,

    /// Size of resolver mask cache
    #[arg(long)]
    #[default(10000)]
    pub resolver_cache: usize,

    /// live time (as second) of resolver mask cache
    #[arg(long)]
    #[default(1)]
    pub resolver_time: u64,

    /// local ident mapping
    #[arg(skip)]
    pub nodes: Vec<Node>,

    /// host command line to run after sidecar started
    #[arg(last = false)]
    #[default(vec![])]
    pub host_cmd: Vec<String>,
}

impl Args {
    pub fn validate(&self) -> Result<()> {
        if self.ident.to_bits() == 0 {
            return Err(Error::new(ErrorKind::InvalidData, "ident required"));
        }
        if self.shmfile.as_os_str().is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "shmfile required",
            ));
        }
        Ok(())
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct Node {
    pub ident: Ipv4Addr,
    pub addr: SocketAddr,
}

fn get_page_size() -> usize {
    page_size::get()
}
