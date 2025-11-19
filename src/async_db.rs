use std::{
    future,
    sync::mpsc::{self, TryRecvError},
    task::Poll,
};

use self_cell::MutBorrow;

use crate::{Database, Transaction, migrate::Schema, transaction::OwnedTransaction};

struct Task<S: 'static>(Box<dyn Send + FnOnce(&'static Transaction<S>)>);

pub struct DatabaseAsync<S: 'static> {
    queue: mpsc::Sender<Task<S>>,
}

impl<S: 'static + Send + Sync + Schema> DatabaseAsync<S> {
    pub fn new(db: Database<S>) -> Self {
        let (queue, queue_recv) = mpsc::channel();

        std::thread::spawn(move || {
            std::thread::scope(|s| {
                loop {
                    let task: Task<S> = queue_recv.recv().unwrap();

                    s.spawn(|| {
                        use r2d2::ManageConnection;
                        let conn = db.manager.connect().unwrap();

                        let owned = OwnedTransaction::new(MutBorrow::new(conn), |conn| {
                            Some(conn.borrow_mut().transaction().unwrap())
                        });

                        let txn = Transaction::new_checked(owned, &db.schema_version);

                        (task.0)(txn);
                    });
                }
            })
        });

        DatabaseAsync { queue }
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

        self.queue
            .send(Task(Box::new(move |txn| {
                let res = f(txn);
                ret_send.send(res).unwrap();
                waker.wake();
            })))
            .unwrap();

        future::poll_fn(|_cx| match ret.try_recv() {
            Ok(val) => Poll::Ready(val),
            Err(TryRecvError::Empty) => Poll::Pending,
            Err(TryRecvError::Disconnected) => panic!(),
        })
        .await
    }
}
