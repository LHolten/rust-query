use std::{
    future,
    mem::replace,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
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

pub struct DoneOnDrop {
    done: Arc<AtomicBool>,
    waker: Waker,
}

impl Drop for DoneOnDrop {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        replace(&mut self.waker, Waker::noop().clone()).wake();
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

        let done_on_drop = DoneOnDrop {
            done: done.clone(),
            waker,
        };
        let db = self.inner.clone();
        let handle = std::thread::spawn(move || {
            // this value will be dropped regardles if the thread finishes normally or with panic
            let _done_on_drop = done_on_drop;
            db.transaction_local(f)
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
