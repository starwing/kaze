use std::alloc::Layout;
use std::fmt;
use std::future::{self, Future};
use std::mem::{self, ManuallyDrop};
use std::pin::Pin;
use std::ptr;
use std::task::{Context, Poll, ready};

/// A reusable `Pin<Box<dyn Future<Output = T> + Send + 'a>>`.
///
/// This type lets you replace the future stored in the box without
/// reallocating when the size and alignment permits this.
pub struct ReusableBoxFuture<T> {
    boxed: Pin<Box<dyn Future<Output = T> + Send>>,
    valid: bool,
}

impl<T> ReusableBoxFuture<T> {
    /// Create a new `ReusableBoxFuture<T>` containing the provided future.
    pub fn new<'a, F>(future: F) -> Self
    where
        F: Future<Output = T> + Send + 'a,
    {
        let boxed: Pin<Box<dyn Future<Output = T>>> = Box::pin(future);
        // SAFETY: erase lifetime marker here, the future may lives shorter than
        // self, but we never touch the memory of the future after the poll()
        // returns Ready.
        let boxed = unsafe { std::mem::transmute(boxed) };
        Self { boxed, valid: true }
    }

    /// Replace the future currently stored in this box.
    ///
    /// This reallocates if and only if the layout of the provided future is
    /// different from the layout of the currently stored future.
    pub fn set<'a, F>(&mut self, future: F)
    where
        F: Future<Output = T> + Send + 'a,
    {
        if let Err(future) = self.try_set(future) {
            *self = Self::new(future);
            self.valid = true;
        }
    }

    /// Replace the future currently stored in this box.
    ///
    /// This function never reallocates, but returns an error if the provided
    /// future has a different size or alignment from the currently stored
    /// future.
    pub fn try_set<'a, F>(&mut self, future: F) -> Result<(), F>
    where
        F: Future<Output = T> + Send + 'a,
    {
        // If we try to inline the contents of this function, the type checker complains because
        // the bound `T: 'a` is not satisfied in the call to `pending()`. But by putting it in an
        // inner function that doesn't have `T` as a generic parameter, we implicitly get the bound
        // `F::Output: 'a` transitively through `F: 'a`, allowing us to call `pending()`.
        #[inline(always)]
        fn real_try_set<'a, F, O>(
            this: &mut ReusableBoxFuture<F::Output>,
            future: F,
        ) -> Result<(), F>
        where
            F: Future<Output = O> + Send + 'a,
        {
            // future::Pending<T> is a ZST so this never allocates.
            let pinned: Pin<Box<dyn Future<Output = O>>> =
                Box::pin(future::pending());
            // SAFETY: erase lifetime marker here, because pending is ZST and
            // never touch memory.
            let pinned = unsafe { std::mem::transmute(pinned) };
            let boxed = mem::replace(&mut this.boxed, pinned);
            reuse_pin_box(boxed, future, |boxed| {
                let pinned: Pin<Box<dyn Future<Output = O> + Send>> =
                    Pin::from(boxed);
                // SAFETY: erase lifetime marker here, the future may lives
                // shorter than self, but we never touch the memory of the
                // future after the poll() returns Ready.
                this.boxed = unsafe { std::mem::transmute(pinned) };
                this.valid = true
            })
        }

        real_try_set(self, future)
    }

    /// Poll the future stored inside this box.
    pub fn poll(&mut self, cx: &mut Context<'_>) -> Poll<T> {
        if !self.valid {
            panic!("poll after ready");
        }
        let result = ready!(self.boxed.as_mut().poll(cx));
        self.valid = false;
        result.into()
    }
}

// The only method called on self.boxed is poll, which takes &mut self, so this
// struct being Sync does not permit any invalid access to the Future, even if
// the future is not Sync.
unsafe impl<T> Sync for ReusableBoxFuture<T> {}

impl<T> fmt::Debug for ReusableBoxFuture<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReusableBoxFuture").finish()
    }
}

fn reuse_pin_box<T: ?Sized, U, O, F>(
    boxed: Pin<Box<T>>,
    new_value: U,
    callback: F,
) -> Result<O, U>
where
    F: FnOnce(Box<U>) -> O,
{
    let layout = Layout::for_value::<T>(&*boxed);
    if layout != Layout::new::<U>() {
        return Err(new_value);
    }

    // SAFETY: We don't ever construct a non-pinned reference to the old `T` from now on, and we
    // always drop the `T`.
    let raw: *mut T =
        Box::into_raw(unsafe { Pin::into_inner_unchecked(boxed) });

    // When dropping the old value panics, we still want to call `callback` — so move the rest of
    // the code into a guard type.
    let guard = CallOnDrop::new(|| {
        let raw: *mut U = raw.cast::<U>();
        unsafe { raw.write(new_value) };

        // SAFETY:
        // - `T` and `U` have the same layout.
        // - `raw` comes from a `Box` that uses the same allocator as this one.
        // - `raw` points to a valid instance of `U` (we just wrote it in).
        let boxed = unsafe { Box::from_raw(raw) };

        callback(boxed)
    });

    // Drop the old value.
    unsafe { ptr::drop_in_place(raw) };

    // Run the rest of the code.
    Ok(guard.call())
}

struct CallOnDrop<O, F: FnOnce() -> O> {
    f: ManuallyDrop<F>,
}

impl<O, F: FnOnce() -> O> CallOnDrop<O, F> {
    fn new(f: F) -> Self {
        let f = ManuallyDrop::new(f);
        Self { f }
    }
    fn call(self) -> O {
        let mut this = ManuallyDrop::new(self);
        let f = unsafe { ManuallyDrop::take(&mut this.f) };
        f()
    }
}

impl<O, F: FnOnce() -> O> Drop for CallOnDrop<O, F> {
    fn drop(&mut self) {
        let f = unsafe { ManuallyDrop::take(&mut self.f) };
        f();
    }
}
#[cfg(test)]
mod tests {
    use std::{sync::Arc, task::Wake};

    use super::*;

    struct MockWaker;
    impl Wake for MockWaker {
        fn wake(self: Arc<Self>) {}
    }

    #[test]
    fn test_reusable_box_future_basic() {
        let waker = Arc::new(MockWaker).into();
        let mut cx = Context::from_waker(&waker);

        let mut box_fut = ReusableBoxFuture::new(async { 42 });
        assert!(matches!(box_fut.poll(&mut cx), Poll::Ready(42)));
    }

    #[test]
    fn test_reusable_box_future_set() {
        let waker = Arc::new(MockWaker).into();
        let mut cx = Context::from_waker(&waker);

        let mut box_fut = ReusableBoxFuture::new(async { 1 });
        assert!(matches!(box_fut.poll(&mut cx), Poll::Ready(1)));

        box_fut.set(async { 2 });
        assert!(matches!(box_fut.poll(&mut cx), Poll::Ready(2)));
    }

    #[test]
    fn test_try_set_same_size() {
        let mut box_fut = ReusableBoxFuture::new(async { "test1" });
        assert!(box_fut.try_set(async { "test2" }).is_ok());
    }

    #[test]
    fn test_try_set_different_size() {
        // Future 1: output type u8, captures a small amount of data or nothing.
        let future1 = async {
            // A simple future that captures very little or nothing.
            0u8
        };

        // Future 2: output type u8, captures a larger amount of data to ensure a different layout.
        let data_to_capture = [0u8; 128]; // This array will be part of future2's state.
        let future2 = async move {
            // Use the captured data to ensure it's part of the future's state.
            // The `move` keyword ensures `data_to_capture` is moved into the future.
            data_to_capture[0]
        };

        // Check that these futures indeed have different layouts.
        // Layout::for_value gets the layout of the actual value.
        let layout1 = std::alloc::Layout::for_value(&future1);
        let layout2 = std::alloc::Layout::for_value(&future2);

        // This assertion is crucial for the test's validity.
        // If the layouts happen to be the same
        // (e.g., due to compiler optimizations or identical captures),
        // the test wouldn't be verifying the intended "different size" scenario.
        assert_ne!(
            layout1, layout2,
            "The layouts of future1 and future2 must be different"
        );

        // Initialize ReusableBoxFuture with future1. The output type T is u8.
        let mut box_fut: ReusableBoxFuture<u8> =
            ReusableBoxFuture::new(future1);

        // Attempt to set future2. Since its layout is different from future1's,
        // try_set should return Err. The output type (u8) is compatible.
        // The Err variant contains the future that could not be set.
        assert!(
            box_fut.try_set(future2).is_err(),
            "should return Err when with different layout."
        );
    }

    #[test]
    #[should_panic(expected = "poll after ready")]
    fn test_poll_after_ready_panics() {
        let waker = Arc::new(MockWaker).into();
        let mut cx = Context::from_waker(&waker);
        let mut box_fut = ReusableBoxFuture::new(async { 42 });

        let _ = box_fut.poll(&mut cx);
        let _ = box_fut.poll(&mut cx); // Should panic
    }
}
