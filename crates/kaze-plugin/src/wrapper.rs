use kaze_util::make_wrapper;

use crate::Plugin;

#[derive(Clone, Copy)]
pub struct NonPlugin<T> {
    inner: T,
}

impl<T> NonPlugin<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
}

make_wrapper!(NonPlugin);

impl<T: Clone + Sync + Send + 'static> Plugin for NonPlugin<T> {
    fn init(&self, _context: crate::Context) {}

    fn context(&self) -> &crate::Context {
        unimplemented!()
    }
}
