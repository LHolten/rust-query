use std::{collections::VecDeque, sync::Mutex};

pub(crate) struct Pool {
    manager: r2d2_sqlite::SqliteConnectionManager,
    reserve: Mutex<VecDeque<rusqlite::Connection>>,
    max_reserve: usize,
}

impl Pool {
    pub fn new(manager: r2d2_sqlite::SqliteConnectionManager) -> Self {
        Self {
            manager,
            reserve: Mutex::new(VecDeque::new()),
            max_reserve: 10,
        }
    }

    /// Get a new connection from the reserve or make a new one.
    pub fn pop(&self) -> rusqlite::Connection {
        self.pop_fast().unwrap_or_else(|| {
            use r2d2::ManageConnection;
            self.manager.connect().unwrap()
        })
    }

    // code optimized to hold lock for shortest time possible
    fn pop_fast(&self) -> Option<rusqlite::Connection> {
        // retrieve the newest connection
        self.reserve.lock().unwrap().pop_front()
    }

    /// Only return connections that are in original condition.
    pub fn push(&self, val: rusqlite::Connection) {
        self.push_fast(val).map(drop);
    }

    // code optimized to hold lock for shortest time possible
    fn push_fast(&self, val: rusqlite::Connection) -> Option<rusqlite::Connection> {
        let mut guard = self.reserve.lock().unwrap();
        let old = if guard.len() >= self.max_reserve {
            // remove the oldest connection
            guard.pop_back()
        } else {
            None
        };
        // push as the newest connection
        guard.push_front(val);
        old
    }
}
