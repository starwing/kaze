use tokio::{sync::Mutex, task::block_in_place};

use crate::{ringbuf::ReceivedData, shm};

pub struct TokioShm {
    shm: shm::Shm,
    push_lock: Mutex<()>,
    pop_lock: Mutex<()>,
}

unsafe impl Send for TokioShm {}
unsafe impl Sync for TokioShm {}

impl TokioShm {
    pub fn new(shm: shm::Shm) -> Self {
        Self {
            shm,
            push_lock: Mutex::new(()),
            pop_lock: Mutex::new(()),
        }
    }

    pub async fn push(&self, data: &[u8]) {
        let _ = self.push_lock.lock().await;
        // SAFETY: use lock to makes push only called by one thread.
        if unsafe { self.shm.try_push(data) } {
            return;
        }

        block_in_place(|| {
            // SAFETY: use lock to makes push only called by one thread.
            unsafe { self.shm.push(data) }
        });
    }

    pub async fn pop(&self) -> ReceivedData<'static> {
        let _ = self.pop_lock.lock().await;
        // SAFETY: use lock to makes pop only called by one thread.
        if let Some(data) = unsafe { self.shm.try_pop() } {
            return data;
        }

        block_in_place(|| {
            // SAFETY: use lock to makes pop only called by one thread.
            unsafe { self.shm.pop() }
        })
    }
}
