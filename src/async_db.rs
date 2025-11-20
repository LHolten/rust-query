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

pub struct WakeOnDrop {
    waker: Option<Waker>,
}

impl WakeOnDrop {
    pub fn new(waker: Waker) -> Self {
        Self { waker: Some(waker) }
    }
}

impl Drop for WakeOnDrop {
    fn drop(&mut self) {
        self.waker.take().unwrap().wake();
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
        let waker = future::poll_fn(|cx| Poll::Ready(cx.waker().clone())).await;
        let done = Arc::new(());
        let done_clone = done.clone();
        let wake_on_drop = WakeOnDrop::new(waker);

        let db = self.inner.clone();
        let handle = std::thread::spawn(move || {
            // waker will be called when thread finishes, even with panic.
            let _wake_on_drop = wake_on_drop;
            // done arc is dropped before waking
            let _done_clone = done_clone;
            db.transaction_local(f)
        });

        // asynchonously wait for the thread to finish
        future::poll_fn(|_cx| {
            // check if the done arc is dropped
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
}
