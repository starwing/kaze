use std::{
    any::TypeId,
    collections::HashMap,
    future::pending,
    hash::{BuildHasherDefault, Hasher},
    sync::{Arc, OnceLock},
};

use tokio::sync::Notify;
use tokio_graceful::{Shutdown, ShutdownGuard};

use kaze_protocol::{
    message::PacketWithAddr,
    packet::{BytesPool, Packet, new_bytes_pool},
    proto::hdr::RouteType,
};
use kaze_util::tower_ext::ServiceExt;

use crate::{PipelineCell, Plugin, local_node};

type AnyMap = HashMap<TypeId, Box<dyn Plugin>, BuildHasherDefault<IdHasher>>;

pub struct ContextBuilder {
    components: AnyMap,
}

impl ContextBuilder {
    pub fn register<T: Plugin>(mut self, component: T) -> Self {
        self.components
            .insert(TypeId::of::<T>(), Box::new(component));
        self
    }

    pub fn build(self) -> Context {
        let ctx = Context::new(self.components);
        for component in ctx.inner.components.values() {
            component.init(ctx.clone());
        }
        ctx
    }
}

#[derive(Clone)]
pub struct Context {
    inner: Arc<Inner>,
}

// shutdown_guard is triggered when the exit signal is received, but at this
// time the whole system may not be prepared to exit yet.
//
// All plugins will received the `shutdown_guard` exit, but they can choose
// ignore it, and using the `notify` signal instead.
//
// Some plugins may trigger the exit signal after the shutdown guard is
// triggered, but if nobody triggers the exit signal after a timeout, the
// system will trigger `exit` automatically.
struct Inner {
    sink: PipelineCell,
    pool: BytesPool,
    components: AnyMap,
    shutdown_guard: OnceLock<ShutdownGuard>,
    real_exit: Notify,
}

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context").finish()
    }
}

impl Context {
    fn new(components: AnyMap) -> Self {
        Self {
            inner: Arc::new(Inner {
                sink: PipelineCell::new(),
                pool: new_bytes_pool(),
                shutdown_guard: OnceLock::new(),
                real_exit: Notify::new(),
                components,
            }),
        }
    }

    pub fn mock() -> Self {
        Self::new(AnyMap::default())
    }

    pub fn builder() -> ContextBuilder {
        ContextBuilder {
            components: AnyMap::default(),
        }
    }

    pub fn set_shutdown_guard(
        &self,
        shutdown_guard: ShutdownGuard,
    ) -> Result<(), ShutdownGuard> {
        self.inner.shutdown_guard.set(shutdown_guard)
    }

    /// Get an exclusive reference to a type previously inserted on this `Extensions`.
    pub fn get_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        Arc::get_mut(&mut self.inner).and_then(|inner| {
            inner
                .components
                .get_mut(&TypeId::of::<T>())
                .and_then(|boxed| (**boxed).as_any_mut().downcast_mut())
        })
    }

    pub fn sink(&self) -> &PipelineCell {
        &self.inner.sink
    }

    pub fn pool(&self) -> &BytesPool {
        &self.inner.pool
    }

    /// Get a reference to the shutdown guard,
    /// if and only if the executor was created with [`Self::graceful`].
    pub fn shutdown_guard(&self) -> Option<&ShutdownGuard> {
        self.inner.shutdown_guard.get()
    }

    /// Returns true if the `Extensions` contains the given type.
    pub fn contains<T: Send + Sync + 'static>(&self) -> bool {
        self.inner.components.contains_key(&TypeId::of::<T>())
    }

    /// Get a shared reference to a type previously inserted on this `Extensions`.
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.inner
            .components
            .get(&TypeId::of::<T>())
            .and_then(|boxed| boxed.as_any().downcast_ref())
    }

    /// Get a list of all components registered in this `Extensions`.
    pub fn components(&self) -> impl IntoIterator<Item = &dyn Plugin> {
        self.inner.components.values().map(|v| v.as_ref())
    }

    /// Waiting for the shutdown guard exiting signal, returns when the process
    /// exit signal (such as Ctrl+C) is received.
    pub async fn shutdwon_triggered(&self) {
        if let Some(guard) = self.shutdown_guard() {
            guard.cancelled().await;
        } else {
            pending::<()>().await;
        }
    }

    /// Waiting for the real exit notify, this is triggered when someone calls
    /// [`Context::trigger_exiting`] after the shutdown guard is triggered.
    pub async fn exiting(&self) {
        self.inner.real_exit.notified().await;
    }

    /// Trigger the exit signal, this is used to notify all plugins that the
    /// system is real shutting down after the shutdown guard is triggered.
    pub fn trigger_exiting(&self) {
        self.inner.real_exit.notify_waiters();
    }

    /// Spawn a future on the current executor,
    /// this is spawned gracefully in case a shutdown guard has been registered.
    pub fn spawn_task<T>(
        &self,
        future: T,
    ) -> tokio::task::JoinHandle<T::Output>
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        if let Some(guard) = self.shutdown_guard() {
            return guard.spawn_task(future);
        }
        tokio::spawn(future)
    }

    pub fn spawn_task_fn<F, T>(
        &self,
        future: F,
    ) -> tokio::task::JoinHandle<T::Output>
    where
        F: FnOnce(ShutdownGuard) -> T + Send + 'static,
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        if let Some(guard) = self.shutdown_guard() {
            return guard.spawn_task_fn(future);
        }
        // TODO: any better way to do this?
        let shutdown = Shutdown::default();
        tokio::spawn(future(shutdown.guard()))
    }

    /// Send message to the pipeline
    pub async fn send(&self, msg: PacketWithAddr) -> anyhow::Result<()> {
        self.sink()
            .clone()
            .ready_call(msg)
            .await
            .map_err(|e| anyhow::anyhow!("failed to send message: {}", e))
    }

    pub async fn send_local(&self, mut msg: Packet) -> anyhow::Result<()> {
        let local = local_node().ident;
        msg.hdr_mut().route_type = Some(RouteType::DstIdent(local));
        msg.hdr_mut().src_ident = local;
        self.sink()
            .clone()
            .ready_call((msg, None))
            .await
            .map_err(|e| anyhow::anyhow!("failed to send message: {}", e))
    }
}

// With TypeIds as keys, there's no need to hash them. They are already hashes
// themselves, coming from the compiler. The IdHasher just holds the u64 of
// the TypeId, and then returns it, instead of doing any bit fiddling.
#[derive(Default)]
struct IdHasher(u64);

impl Hasher for IdHasher {
    fn write(&mut self, _: &[u8]) {
        unreachable!("TypeId calls write_u64");
    }

    #[inline]
    fn write_u64(&mut self, id: u64) {
        self.0 = id;
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;
    use std::task::{Context as TaskContext, Poll};

    use crate::{Context, Plugin};

    #[derive(Clone)]
    struct TestComponent {
        value: String,
    }

    impl Plugin for TestComponent {}

    struct TestFuture {
        complete: bool,
    }

    impl Future for TestFuture {
        type Output = &'static str;

        fn poll(
            mut self: Pin<&mut Self>,
            _cx: &mut TaskContext<'_>,
        ) -> Poll<Self::Output> {
            if self.complete {
                Poll::Ready("done")
            } else {
                self.complete = true;
                Poll::Pending
            }
        }
    }

    #[tokio::test]
    async fn test_register_and_get_component() {
        let context = Context::builder()
            .register(TestComponent {
                value: "test".to_string(),
            })
            .build();
        let retrieved = context.get::<TestComponent>().unwrap();
        assert_eq!(retrieved.value, "test");
    }

    #[tokio::test]
    async fn test_get_nonexistent_component() {
        let context = Context::builder().build();

        let retrieved = context.get::<TestComponent>();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_spawn_task() {
        let context = Context::builder().build();

        let handle = context.spawn_task(async { 42 });

        assert_eq!(handle.await.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_spawn_task_fn() {
        let context = Context::builder().build();
        let handle = context.spawn_task_fn(|_guard| async { 42 });
        assert_eq!(handle.await.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_guard_reference() {
        let context = Context::builder().build();

        let handle = context.spawn_task_fn(|_guard| async { 42 });
        assert_eq!(handle.await.unwrap(), 42);
    }
}
