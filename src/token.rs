use std::{any::Any, cell::Cell, sync::Arc};

use rusqlite::Connection;

/// Only one [ThreadToken] exists in each thread.
/// It can thus not be send across threads.
pub struct ThreadToken {
    _p: std::marker::PhantomData<*const ()>,
    pub(crate) stuff: Arc<dyn Any>,
    pub(crate) conn: Option<Connection>,
}

thread_local! {
    static EXISTS: Cell<bool> = const { Cell::new(true) };
}

impl ThreadToken {
    fn new() -> Self {
        ThreadToken {
            _p: std::marker::PhantomData,
            stuff: Arc::new(()),
            conn: None,
        }
    }

    /// Create a [ThreadToken] if it was created not created yet on this thread.
    ///
    /// Async tasks often share their thread and can thus not use this method.
    /// Instead you should use your equivalent of `spawn_blocking` or `block_in_place`.
    /// These functions guarantee that you have a unique thread and thus allow [ThreadToken::try_new].
    ///
    /// Note that using `spawn_blocking` for sqlite is actually a good practice.
    /// Sqlite will essentially do blocking IO every time it is called.
    /// Doing so on all async runtime threads would prevent other tasks from executing.
    pub fn try_new() -> Option<Self> {
        EXISTS.replace(false).then(ThreadToken::new)
    }
}

impl Drop for ThreadToken {
    fn drop(&mut self) {
        EXISTS.set(true)
    }
}
