use std::time::Duration;

use kaze_resolver::Resolver;

use crate::{Corral, Options, RateLimit};

/// builder for corral
pub struct Builder<R: Resolver> {
    /// options for corral
    pub(crate) options: Options,

    /// rate limit for connection
    pub(crate) rate_limit: Option<RateLimit>,

    /// resolver
    pub(crate) resolver: R,

    /// sender
    pub(crate) sender: kaze_edge::Sender,
}

impl<R: Resolver> Builder<R> {
    /// create a new builder
    #[allow(dead_code)]
    pub fn new(resolver: R, sender: kaze_edge::Sender) -> Self {
        Self {
            options: Options::default(),
            rate_limit: None,
            resolver,
            sender,
        }
    }

    /// create a new builder from options
    pub fn from_options(
        options: Options,
        resolver: R,
        sender: kaze_edge::Sender,
    ) -> Self {
        Self {
            options,
            rate_limit: None,
            resolver,
            sender,
        }
    }

    /// set options
    pub fn with_options(mut self, options: Options) -> Self {
        self.options = options;
        self
    }

    /// set pending timeout
    pub fn with_pending_timeout(mut self, pending_timeout: Duration) -> Self {
        self.options.pending_timeout = pending_timeout.into();
        self
    }

    /// set idle timeout
    pub fn with_idle_timeout(mut self, idle_timeout: Duration) -> Self {
        self.options.idle_timeout = idle_timeout.into();
        self
    }

    /// set rate limit
    pub fn with_rate_limit(mut self, rate_limit: Option<RateLimit>) -> Self {
        self.rate_limit = rate_limit;
        self
    }

    /// build corral
    pub fn build(self, ident: u32) -> Corral<R> {
        Corral::new(self, ident)
    }
}
