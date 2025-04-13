use clap;

pub trait ClapDefault: clap::Args {
    /// Get the default config from clap
    fn default() -> Self {
        let mut cmd = clap::Command::new("__dummy__");
        cmd = Self::augment_args(cmd);
        let matches = cmd.get_matches_from(vec!["__dummy__"]);
        Self::from_arg_matches(&matches)
            .expect("Failed to get default config from clap")
    }
}

impl<T> ClapDefault for T where T: clap::Args {}
