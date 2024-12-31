use clap::ArgMatches;

pub use clap_merge_derive::ClapMerge;

pub trait ClapMerge {
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
