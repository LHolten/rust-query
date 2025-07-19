use std::cell::Cell;

/// The primary interface to the database.
///
/// Only one [LocalClient] can exist in each thread and transactions need to mutably borrow a [LocalClient].
/// This makes it impossible to have access to two transactions from one thread.
///
/// The only way to have concurrent read transactions is to have them on different threads.
/// Write transactions never run in parallell with each other, but they do run in parallel with read transactions.
pub struct LocalClient {
    _p: std::marker::PhantomData<*const ()>,
}

thread_local! {
    static EXISTS: Cell<bool> = const { Cell::new(true) };
}

impl LocalClient {
    fn new() -> Self {
        LocalClient {
            _p: std::marker::PhantomData,
        }
    }

    /// Create a [LocalClient] if it was not created yet on this thread.
    ///
    /// Async tasks often share their thread and can thus not use this method.
    /// Instead you should use your equivalent of `spawn_blocking` or `block_in_place`.
    /// These functions guarantee that you have a unique thread and thus allow [LocalClient::try_new].
    ///
    /// Note that using `spawn_blocking` for sqlite is actually a good practice.
    /// Sqlite queries can be expensive, it might need to read from disk which is slow.
    /// Doing so on all async runtime threads would prevent other tasks from executing.
    pub fn try_new() -> Option<Self> {
        EXISTS.replace(false).then(LocalClient::new)
    }
}

impl Drop for LocalClient {
    /// Dropping a [LocalClient] allows retrieving it with [LocalClient::try_new] again.
    fn drop(&mut self) {
        EXISTS.set(true)
    }
}
