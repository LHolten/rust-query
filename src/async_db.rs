use std::{
    future,
    sync::{
        self, Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::Poll,
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
        let done = Arc::new(AtomicBool::new(false));

        let done_clone = done.clone();
        let db = self.inner.clone();
        let handle = std::thread::spawn(move || {
            let res = db.transaction_local(f);
            done_clone.store(true, sync::atomic::Ordering::SeqCst);
            waker.wake();
            res
        });

        future::poll_fn(|_cx| {
            if done.load(Ordering::SeqCst) {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;

        match handle.join() {
            Ok(val) => val,
            Err(err) => std::panic::resume_unwind(err),
        }
    }
}
