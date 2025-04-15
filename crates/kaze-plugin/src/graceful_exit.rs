use std::sync::{Arc, atomic::AtomicBool};

use tokio::{
    select,
    sync::{Notify, futures::Notified},
};

pub struct GracefulExit {
    inner: Arc<GracefulExitInner>,
}

pub enum State {
    /// The process is exiting.
    Exiting,
    /// The process has exited.
    Exited,
}

struct GracefulExitInner {
    start_notify: Notify,
    finish_notify: Notify,
    is_exiting: AtomicBool,
}

impl GracefulExit {
    pub fn new() -> Self {
        let inner = Arc::new(GracefulExitInner {
            start_notify: Notify::new(),
            finish_notify: Notify::new(),
            is_exiting: AtomicBool::new(false),
        });
        Self { inner }
    }

    pub fn notify_start(&self) {
        self.inner
            .is_exiting
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.inner.start_notify.notify_waiters();
    }

    pub fn notify_finish(&self) {
        self.inner.finish_notify.notify_waiters();
    }

    pub async fn notified(&self) -> State {
        select! {
            _ = self.inner.start_notify.notified() => {
                State::Exiting
            }
            _ = self.inner.finish_notify.notified() => {
                State::Exited
            }
        }
    }

    pub fn start_notified(&self) -> Notified {
        self.inner.start_notify.notified()
    }

    pub fn finish_notified(&self) -> Notified {
        self.inner.finish_notify.notified()
    }

    pub fn is_exiting(&self) -> bool {
        self.inner
            .is_exiting
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}
