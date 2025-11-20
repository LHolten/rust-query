use std::{
    future,
    sync::Arc,
    task::{Poll, Waker},
};

use crate::{Database, Transaction, migrate::Schema};

pub struct DatabaseAsync<S> {
    inner: Arc<Database<S>>,
}

impl<S> Clone for DatabaseAsync<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

async fn async_run<R: 'static + Send>(f: impl 'static + Send + FnOnce() -> R) -> R {
    pub struct WakeOnDrop {
        waker: Option<Waker>,
    }

    impl Drop for WakeOnDrop {
        fn drop(&mut self) {
            self.waker.take().unwrap().wake();
        }
    }

    let waker = future::poll_fn(|cx| Poll::Ready(cx.waker().clone())).await;
    let done = Arc::new(());

    let handle = std::thread::spawn({
        let done = done.clone();
        move || {
            // waker will be called when thread finishes, even with panic.
            let _wake_on_drop = WakeOnDrop { waker: Some(waker) };
            // done arc is dropped before waking
            let _done_on_drop = done;
            f()
        }
    });

    // asynchonously wait for the thread to finish
    future::poll_fn(|_cx| {
        // check if the done Arc is dropped
        if Arc::strong_count(&done) == 1 {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;

    // we know that the thread is finished, so we block on it
    match handle.join() {
        Ok(val) => val,
        Err(err) => std::panic::resume_unwind(err),
    }
}

impl<S: 'static + Send + Sync + Schema> DatabaseAsync<S> {
    pub fn new(db: Arc<Database<S>>) -> Self {
        DatabaseAsync { inner: db }
    }

    /// This is a lot like [Database::transaction], only difference is that the async function
    /// does not block the runtime and requires the closure to be `'static`.
    /// The static requirement is because the future may be canceled, but the transaction can not
    /// be canceled.
    pub async fn transaction<R: 'static + Send>(
        &self,
        f: impl 'static + Send + FnOnce(&'static Transaction<S>) -> R,
    ) -> R {
        let db = self.inner.clone();
        async_run(move || db.transaction_local(f)).await
    }

    pub async fn transaction_mut<O: 'static + Send, E: 'static + Send>(
        &self,
        f: impl 'static + Send + FnOnce(&'static mut Transaction<S>) -> Result<O, E>,
    ) -> Result<O, E> {
        let db = self.inner.clone();
        async_run(move || db.transaction_mut_local(f)).await
    }
}
