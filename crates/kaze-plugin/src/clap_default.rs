use clap;

/// This module provides utilities for working with `clap`
pub fn default_from_clap<T: clap::Args>() -> T {
    let mut cmd = clap::Command::new("__dummy__");
    cmd = T::augment_args(cmd);
    let matches = cmd.get_matches_from(vec!["__dummy__"]);
    T::from_arg_matches(&matches)
        .expect("Failed to get default config from clap")
}
