// modified from https://github.com/PureWhiteWu/async_singleflight

//! A singleflight implementation for tokio.
//!
//! Inspired by [singleflight](https://crates.io/crates/singleflight).
//!
//! # Examples
//!
//! ```no_run
//! use futures::future::join_all;
//! use std::sync::Arc;
//! use std::time::Duration;
//!
//! use async_singleflight::Group;
//!
//! const RES: usize = 7;
//!
//! async fn expensive_fn() -> Result<usize, ()> {
//!     tokio::time::sleep(Duration::new(1, 500)).await;
//!     Ok(RES)
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let g = Arc::new(Group::<_, ()>::new());
//!     let mut handlers = Vec::new();
//!     for _ in 0..10 {
//!         let g = g.clone();
//!         handlers.push(tokio::spawn(async move {
//!             let res = g.work("key", expensive_fn()).await.0;
//!             let r = res.unwrap().unwrap();
//!             println!("{}", r);
//!         }));
//!     }
//!
//!     join_all(handlers).await;
//! }
//! ```
//!

use std::fmt::{self, Debug};
use std::future::Future;
use std::hash::Hash;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use parking_lot::Mutex;
use pin_project::{pin_project, pinned_drop};
use std::collections::HashMap;
use tokio::sync::watch;

/// Group represents a class of work and creates a space in which units of work
/// can be executed with duplicate suppression.
pub struct Group<K, V, E> {
    m: Mutex<HashMap<K, watch::Receiver<State<V>>>>,
    _marker: PhantomData<fn(E)>,
}

impl<K, V, E> Debug for Group<K, V, E>
where
    K: Hash + Eq,
    V: Clone,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Group").finish()
    }
}

impl<K, V, E> Default for Group<K, V, E>
where
    K: Clone + Hash + Eq,
    V: Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
enum State<T> {
    Starting,
    LeaderDropped,
    Done(Option<T>),
}

impl<K, V, E> Group<K, V, E> {
    /// Create a new Group to do work with.
    #[must_use]
    pub fn new() -> Group<K, V, E> {
        Self {
            m: Mutex::new(HashMap::new()),
            _marker: PhantomData,
        }
    }
}

impl<K, V, E> Group<K, V, E>
where
    K: Clone + Hash + Eq,
    V: Clone,
{
    /// Execute and return the value for a given function, making sure that only one
    /// operation is in-flight at a given moment. If a duplicate call comes in, that caller will
    /// wait until the original call completes and return the same value.
    /// Only owner call returns error if exists.
    /// The third return value indicates whether the call is the owner.
    pub async fn work(
        &self,
        key: K,
        fut: impl Future<Output = Result<V, E>>,
    ) -> Result<Result<V, E>, Option<V>> {
        use std::collections::hash_map::Entry;
        loop {
            let tx_or_rx = match self.m.lock().entry(key.clone()) {
                Entry::Occupied(mut entry) => {
                    let state = entry.get().borrow().clone();
                    match state {
                        State::Starting => Err(entry.get().clone()),
                        State::LeaderDropped => {
                            // switch into leader if leader dropped
                            let (tx, rx) = watch::channel(State::Starting);
                            entry.insert(rx);
                            Ok(tx)
                        }
                        State::Done(val) => return Err(val),
                    }
                }
                Entry::Vacant(entry) => {
                    let (tx, rx) = watch::channel(State::Starting);
                    entry.insert(rx);
                    Ok(tx)
                }
            };

            match tx_or_rx {
                Ok(tx) => {
                    let fut = Leader { fut, tx };
                    let result = fut.await;
                    self.m.lock().remove(&key);
                    return Ok(result);
                }
                Err(mut rx) => {
                    let mut state = rx.borrow_and_update().clone();
                    if matches!(state, State::Starting) {
                        let _changed = rx.changed().await;
                        state = rx.borrow().clone();
                    }
                    match state {
                        State::Starting => {
                            unreachable!("state should not be starting")
                        }
                        State::LeaderDropped => {
                            self.m.lock().remove(&key);
                            continue; // retry
                        }
                        State::Done(val) => return Err(val),
                    }
                }
            }
        }
    }
}

#[pin_project(PinnedDrop)]
struct Leader<T: Clone, F> {
    #[pin]
    fut: F,
    tx: watch::Sender<State<T>>,
}

impl<T, E, F> Future for Leader<T, F>
where
    T: Clone,
    F: Future<Output = Result<T, E>>,
{
    type Output = Result<T, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let result = this.fut.poll(cx);
        if let Poll::Ready(val) = &result {
            let _send = this.tx.send(State::Done(val.as_ref().ok().cloned()));
        }
        result
    }
}

#[pinned_drop]
impl<T: Clone, F> PinnedDrop for Leader<T, F> {
    fn drop(self: Pin<&mut Self>) {
        let this = self.project();
        let _ = this.tx.send_if_modified(|s| {
            if matches!(s, State::Starting) {
                *s = State::LeaderDropped;
                true
            } else {
                false
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::Group;

    const RES: usize = 7;

    async fn return_res() -> Result<usize, ()> {
        Ok(7)
    }

    async fn expensive_fn() -> Result<usize, ()> {
        tokio::time::sleep(Duration::from_millis(500)).await;
        Ok(RES)
    }

    #[tokio::test]
    async fn test_simple() {
        let g = Group::new();
        let res = g.work("key", return_res()).await;
        assert_eq!(res, Ok(Ok(RES)));
    }

    #[tokio::test]
    async fn test_multiple_threads() {
        use std::sync::Arc;

        use futures::future::join_all;

        let g = Arc::new(Group::new());
        let mut handlers = Vec::new();
        for _ in 0..10 {
            let g = g.clone();
            handlers.push(tokio::spawn(async move {
                g.work("key", expensive_fn()).await
            }));
        }

        let mut r = vec![Err(Some(RES)); 10];
        r[0] = Ok(Ok(RES));
        let join_res = join_all(handlers)
            .await
            .into_iter()
            .map(|v| v.unwrap())
            .collect::<Vec<_>>();
        assert_eq!(join_res, r);
    }

    #[tokio::test]
    async fn test_drop_leader() {
        use std::time::Duration;

        let g = Group::new();
        {
            tokio::time::timeout(
                Duration::from_millis(50),
                g.work("key", expensive_fn()),
            )
            .await
            .expect_err("owner should be running and cancelled");
        }
        assert_eq!(
            tokio::time::timeout(
                Duration::from_secs(1),
                g.work("key", expensive_fn())
            )
            .await,
            Ok(Ok(Ok(RES))),
        );
    }
}
