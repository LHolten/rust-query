use std::{
    future,
    sync::{
        Arc,
        mpsc::{self, TryRecvError},
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
        let (ret_send, ret) = mpsc::channel();

        let db = self.inner.clone();
        std::thread::spawn(move || {
            db.transaction_local(|txn| {
                let res = f(txn);
                ret_send.send(res).unwrap();
                waker.wake();
            })
        });

        future::poll_fn(|_cx| match ret.try_recv() {
            Ok(val) => Poll::Ready(val),
            Err(TryRecvError::Empty) => Poll::Pending,
            Err(TryRecvError::Disconnected) => panic!(),
        })
        .await
    }
}
