use kaze_plugin::{Plugin, util::make_wrapper};

use crate::{Resolver, resolver_wrapper};

#[derive(Clone, Copy)]
pub struct ResolverPlugin<R> {
    resolver: R,
}

resolver_wrapper!(ResolverPlugin);

impl<R: Resolver> ResolverPlugin<R> {
    pub fn new(resolver: R) -> Self {
        Self { resolver }
    }
}

impl<R: Plugin + Clone> Plugin for ResolverPlugin<R> {
    fn init(self: &Self, context: kaze_plugin::Context) {
        self.resolver.init(context.clone());
    }

    fn context(self: &Self) -> &kaze_plugin::Context {
        self.resolver.context()
    }
}

#[derive(Clone, Copy)]
pub struct ResolverNoPlugin<R> {
    resolver: R,
}

resolver_wrapper!(ResolverNoPlugin);

impl<R: Resolver> ResolverNoPlugin<R> {
    pub fn new(resolver: R) -> Self {
        Self { resolver }
    }
}

impl<R: Clone + Send + Sync + 'static> Plugin for ResolverNoPlugin<R> {
    fn init(self: &Self, _context: kaze_plugin::Context) {}

    fn context(self: &Self) -> &kaze_plugin::Context {
        unimplemented!("ResolverNoPlugin does not have a context")
    }
}
