use std::{any::Any, sync::Arc};

/// Only one [ThreadToken] exists in each thread.
/// It can thus not be send across threads.
pub struct ThreadToken {
    #[cfg(feature = "thread")]
    _p: std::marker::PhantomData<*const ()>,
    pub(crate) stuff: Arc<dyn Any>,
}

impl ThreadToken {
    fn new() -> Self {
        ThreadToken {
            #[cfg(feature = "thread")]
            _p: std::marker::PhantomData,
            stuff: Arc::new(()),
        }
    }
}

#[cfg(feature = "thread")]
mod use_threads {
    use std::cell::Cell;

    use super::*;

    thread_local! {
        static EXISTS: Cell<bool> = const { Cell::new(true) };
    }

    impl ThreadToken {
        /// Retrieve the [ThreadToken] if it was not retrieved yet on this thread.
        pub fn acquire() -> Option<Self> {
            EXISTS.replace(false).then(ThreadToken::new)
        }
    }

    impl Drop for ThreadToken {
        fn drop(&mut self) {
            EXISTS.set(true)
        }
    }
}

#[cfg(feature = "tokio")]
mod use_tokio {
    use std::future::Future;

    use super::*;

    tokio::task_local! {
        static EXISTS: ();
    }

    impl ThreadToken {
        /// Retrieve the [ThreadToken] if it was not retrieved yet in this task.
        pub async fn acquire<F>(f: impl FnOnce(Option<&mut Self>) -> F) -> F::Output
        where
            F: Future,
        {
            match EXISTS.try_with(|&()| ()).is_err() {
                false => f(None).await,
                true => EXISTS.scope((), f(Some(&mut ThreadToken::new()))).await,
            }
        }

        pub fn acquire_sync<R>(f: impl FnOnce(Option<&mut Self>) -> R) -> R {
            match EXISTS.try_with(|&()| ()).is_err() {
                false => f(None),
                true => EXISTS.sync_scope((), || f(Some(&mut ThreadToken::new()))),
            }
        }
    }
}
