use std::sync::Mutex;

pub(crate) struct Pool {
    manager: r2d2_sqlite::SqliteConnectionManager,
    reserve: Mutex<Vec<rusqlite::Connection>>,
    max_reserve: usize,
}

impl Pool {
    pub fn new(manager: r2d2_sqlite::SqliteConnectionManager) -> Self {
        Self {
            manager,
            reserve: Mutex::new(Vec::new()),
            max_reserve: 5,
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
        self.reserve.lock().unwrap().pop()
    }

    /// Only return connections that are in original condition.
    pub fn push(&self, val: rusqlite::Connection) {
        self.push_fast(val).map(drop);
    }

    // code optimized to hold lock for shortest time possible
    fn push_fast(&self, val: rusqlite::Connection) -> Option<rusqlite::Connection> {
        let mut guard = self.reserve.lock().unwrap();
        if guard.len() < self.max_reserve {
            guard.push(val);
            None
        } else {
            Some(val)
        }
    }
}
