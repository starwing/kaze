pub mod sink {
    use std::{
        future::Future, pin::{pin, Pin},
        task::{Context, Poll},
    };
    use anyhow::Result;
    use pin_project_lite::pin_project;
    /// the sink trait for kaze edge and corral. using custom trait but not
    /// `futures::Sink` because we need to `poll_ready` with Message length, that's
    /// not supported by `futures::Sink`.
    pub trait Sink<Item> {
        type Error;
        type Future: Future<Output = Result<(), Self::Error>>;
        fn send(&mut self, item: Item) -> Self::Future;
    }
    pub fn sink_fn<T, F, Item, E>(f: T) -> SinkFn<T>
    where
        T: FnMut(Item) -> F,
        F: Future<Output = Result<(), E>>,
    {
        SinkFn::new(f)
    }
    pub struct SinkFn<T> {
        f: T,
    }
    impl<T> SinkFn<T> {
        pub fn new(f: T) -> Self {
            SinkFn { f }
        }
    }
    impl<T, F, Item, E> Sink<Item> for SinkFn<T>
    where
        T: FnMut(Item) -> F,
        F: Future<Output = Result<(), E>>,
    {
        type Error = E;
        type Future = F;
        fn send(&mut self, item: Item) -> Self::Future {
            (self.f)(item)
        }
    }
    pub struct SinkWrapper<Item, S: Sink<Item>> {
        sink: S,
        state: State<S::Future>,
    }
    #[allow(
        explicit_outlives_requirements,
        single_use_lifetimes,
        clippy::unknown_clippy_lints,
        clippy::absolute_paths,
        clippy::min_ident_chars,
        clippy::redundant_pub_crate,
        clippy::single_char_lifetime_names,
        clippy::used_underscore_binding
    )]
    const _: () = {
        #[doc(hidden)]
        #[allow(
            dead_code,
            single_use_lifetimes,
            clippy::unknown_clippy_lints,
            clippy::absolute_paths,
            clippy::min_ident_chars,
            clippy::mut_mut,
            clippy::redundant_pub_crate,
            clippy::ref_option_ref,
            clippy::single_char_lifetime_names,
            clippy::type_repetition_in_bounds
        )]
        pub(crate) struct Projection<'__pin, Item, S: Sink<Item>>
        where
            SinkWrapper<Item, S>: '__pin,
        {
            sink: &'__pin mut (S),
            state: ::pin_project_lite::__private::Pin<&'__pin mut (State<S::Future>)>,
        }
        #[doc(hidden)]
        #[allow(
            dead_code,
            single_use_lifetimes,
            clippy::unknown_clippy_lints,
            clippy::absolute_paths,
            clippy::min_ident_chars,
            clippy::mut_mut,
            clippy::redundant_pub_crate,
            clippy::ref_option_ref,
            clippy::single_char_lifetime_names,
            clippy::type_repetition_in_bounds
        )]
        pub(crate) struct ProjectionRef<'__pin, Item, S: Sink<Item>>
        where
            SinkWrapper<Item, S>: '__pin,
        {
            sink: &'__pin (S),
            state: ::pin_project_lite::__private::Pin<&'__pin (State<S::Future>)>,
        }
        impl<Item, S: Sink<Item>> SinkWrapper<Item, S> {
            #[doc(hidden)]
            #[inline]
            pub(crate) fn project<'__pin>(
                self: ::pin_project_lite::__private::Pin<&'__pin mut Self>,
            ) -> Projection<'__pin, Item, S> {
                unsafe {
                    let Self { sink, state } = self.get_unchecked_mut();
                    Projection {
                        sink: sink,
                        state: ::pin_project_lite::__private::Pin::new_unchecked(state),
                    }
                }
            }
            #[doc(hidden)]
            #[inline]
            pub(crate) fn project_ref<'__pin>(
                self: ::pin_project_lite::__private::Pin<&'__pin Self>,
            ) -> ProjectionRef<'__pin, Item, S> {
                unsafe {
                    let Self { sink, state } = self.get_ref();
                    ProjectionRef {
                        sink: sink,
                        state: ::pin_project_lite::__private::Pin::new_unchecked(state),
                    }
                }
            }
        }
        #[allow(non_snake_case)]
        pub struct __Origin<'__pin, Item, S: Sink<Item>> {
            __dummy_lifetime: ::pin_project_lite::__private::PhantomData<&'__pin ()>,
            sink: ::pin_project_lite::__private::AlwaysUnpin<S>,
            state: State<S::Future>,
        }
        impl<'__pin, Item, S: Sink<Item>> ::pin_project_lite::__private::Unpin
        for SinkWrapper<Item, S>
        where
            ::pin_project_lite::__private::PinnedFieldsOf<
                __Origin<'__pin, Item, S>,
            >: ::pin_project_lite::__private::Unpin,
        {}
        trait MustNotImplDrop {}
        #[allow(clippy::drop_bounds, drop_bounds)]
        impl<T: ::pin_project_lite::__private::Drop> MustNotImplDrop for T {}
        impl<Item, S: Sink<Item>> MustNotImplDrop for SinkWrapper<Item, S> {}
        #[forbid(unaligned_references, safe_packed_borrows)]
        fn __assert_not_repr_packed<Item, S: Sink<Item>>(this: &SinkWrapper<Item, S>) {
            let _ = &this.sink;
            let _ = &this.state;
        }
    };
    enum State<Fut> {
        /// idle state, ready to accept new items
        Idle,
        /// sending state, holding a future
        Sending { fut: Fut },
        /// closed state
        Closed,
    }
    #[doc(hidden)]
    #[allow(
        dead_code,
        single_use_lifetimes,
        clippy::unknown_clippy_lints,
        clippy::absolute_paths,
        clippy::min_ident_chars,
        clippy::mut_mut,
        clippy::redundant_pub_crate,
        clippy::ref_option_ref,
        clippy::single_char_lifetime_names,
        clippy::type_repetition_in_bounds
    )]
    enum StateProj<'__pin, Fut>
    where
        State<Fut>: '__pin,
    {
        Idle,
        Sending { fut: ::pin_project_lite::__private::Pin<&'__pin mut (Fut)> },
        Closed,
    }
    #[allow(
        single_use_lifetimes,
        clippy::unknown_clippy_lints,
        clippy::absolute_paths,
        clippy::min_ident_chars,
        clippy::single_char_lifetime_names,
        clippy::used_underscore_binding
    )]
    const _: () = {
        impl<Fut> State<Fut> {
            #[doc(hidden)]
            #[inline]
            fn project<'__pin>(
                self: ::pin_project_lite::__private::Pin<&'__pin mut Self>,
            ) -> StateProj<'__pin, Fut> {
                unsafe {
                    match self.get_unchecked_mut() {
                        Self::Idle => StateProj::Idle,
                        Self::Sending { fut } => {
                            StateProj::Sending {
                                fut: ::pin_project_lite::__private::Pin::new_unchecked(fut),
                            }
                        }
                        Self::Closed => StateProj::Closed,
                    }
                }
            }
        }
        #[allow(non_snake_case)]
        struct __Origin<'__pin, Fut> {
            __dummy_lifetime: ::pin_project_lite::__private::PhantomData<&'__pin ()>,
            Idle: (),
            Sending: (Fut),
            Closed: (),
        }
        impl<'__pin, Fut> ::pin_project_lite::__private::Unpin for State<Fut>
        where
            ::pin_project_lite::__private::PinnedFieldsOf<
                __Origin<'__pin, Fut>,
            >: ::pin_project_lite::__private::Unpin,
        {}
        trait MustNotImplDrop {}
        #[allow(clippy::drop_bounds, drop_bounds)]
        impl<T: ::pin_project_lite::__private::Drop> MustNotImplDrop for T {}
        impl<Fut> MustNotImplDrop for State<Fut> {}
    };
    impl<Item, S: Sink<Item>> SinkWrapper<Item, S> {
        /// create a new SinkWrapper
        pub fn new(sink: S) -> Self {
            Self {
                sink,
                state: State::<S::Future>::Idle,
            }
        }
    }
    impl<Item, S> futures::Sink<Item> for SinkWrapper<Item, S>
    where
        S: Sink<Item> + Unpin,
        S::Future: Unpin,
        S::Error: Into<anyhow::Error>,
    {
        type Error = anyhow::Error;
        fn poll_ready(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            let self_projected = self.project();
            loop {
                match self_projected.state.project() {
                    StateProj::Idle => return Poll::Ready(Ok(())),
                    StateProj::Sending { fut } => {
                        match fut.poll(cx) {
                            Poll::Ready(res) => {
                                res.map_err(Into::into)?;
                                break;
                            }
                            Poll::Pending => return Poll::Pending,
                        }
                    }
                    StateProj::Closed => {
                        return Poll::Ready(
                            Err(
                                ::anyhow::__private::must_use({
                                        let error = ::anyhow::__private::format_err(
                                            format_args!("Sink is closed"),
                                        );
                                        error
                                    })
                                    .into(),
                            ),
                        );
                    }
                }
            }
            self_projected.state = State::Idle;
            Poll::Ready(Ok(()))
        }
        fn start_send(mut self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
            match self.state {
                State::Idle => {
                    let fut = self.sink.send(item);
                    self.get_mut().state = State::Sending { fut };
                    Ok(())
                }
                _ => {
                    Err(
                        ::anyhow::__private::must_use({
                                let error = ::anyhow::__private::format_err(
                                    format_args!("Sink not ready"),
                                );
                                error
                            })
                            .into(),
                    )
                }
            }
        }
        fn poll_flush(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            let mut self_projected = self.project();
            match self_projected.state.project() {
                StateProj::Sending { fut } => {
                    match fut.poll(cx) {
                        Poll::Ready(res) => {
                            self_projected.state = Pin::new(&mut State::Idle);
                            res.map_err(Into::into)?;
                            Poll::Ready(Ok(()))
                        }
                        Poll::Pending => Poll::Pending,
                    }
                }
                _ => Poll::Ready(Ok(())),
            }
        }
        fn poll_close(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            let self_projected = self.project();
            loop {
                match self_projected.state.project() {
                    StateProj::Idle => {
                        self.state = State::Closed;
                        return Poll::Ready(Ok(()));
                    }
                    StateProj::Sending { fut } => {
                        match fut.poll(cx) {
                            Poll::Ready(res) => {
                                res.map_err(Into::into)?;
                                std::mem::replace(
                                    self_projected.state.as_mut(),
                                    State::Idle,
                                );
                            }
                            Poll::Pending => return Poll::Pending,
                        }
                    }
                    StateProj::Closed => return Poll::Ready(Ok(())),
                }
            }
        }
    }
}
