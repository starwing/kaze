use clap::ArgMatches;

pub use clap;
pub use clap_merge_derive::ClapMerge;

pub trait ClapMerge {
    /// merge the arguments from ArgMatches into self, return true if self has
    /// been updated
    fn merge(&mut self, args: &ArgMatches) -> bool;
}

impl<T: ClapMerge + Default> ClapMerge for Option<T> {
    fn merge(&mut self, args: &ArgMatches) -> bool {
        if let Some(v) = self.as_mut() {
            return v.merge(args);
        }
        let mut v = T::default();
        if v.merge(args) {
            self.replace(v);
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, path::PathBuf};

    use super::*;

    #[derive(ClapMerge, clap::Parser)]
    struct Test {
        /// Name of config file (default: sidecar.toml)
        #[arg(short, long, default_value = "sidecar.toml")]
        #[arg(value_name = "PATH")]
        pub config: PathBuf,

        /// prometheus metrics endpoint
        #[arg(
            id = "metrics",
            long = "metrics",
            default_missing_value = default_metrics_listening()
        )]
        #[arg(value_name = "ADDR")]
        pub listen: Option<SocketAddr>,

        /// prometheus push endpoint
        #[arg(long = "metrics-push-endpoint")]
        #[arg(value_name = "ADDR")]
        pub endpoint: Option<String>,
    }

    fn default_metrics_listening() -> &'static str {
        "127.0.0.1:9090"
    }

    #[test]
    fn virify_cli() {
        use clap::CommandFactory;
        Test::command().debug_assert();
    }
}
