use std::sync::Mutex;

use crate::migration::Config;

pub(crate) struct Pool {
    config: Config,
    reserve: Mutex<Vec<rusqlite::Connection>>,
    max_reserve: usize,
    // extra connection to keep in memory databases alive
    _anchor: Mutex<rusqlite::Connection>,
}

impl Pool {
    pub fn new(config: Config) -> Self {
        Self {
            _anchor: Mutex::new(config.connect().unwrap()),
            config,
            reserve: Mutex::new(Vec::new()),
            max_reserve: 5,
        }
    }

    /// Get a new connection from the reserve or make a new one.
    pub fn pop(&self) -> rusqlite::Connection {
        self.pop_fast()
            .unwrap_or_else(|| self.config.connect().unwrap())
    }

    // code optimized to hold lock for shortest time possible
    #[cfg_attr(feature = "__mutants", mutants::skip)]
    fn pop_fast(&self) -> Option<rusqlite::Connection> {
        self.reserve.lock().unwrap().pop()
    }

    /// Only return connections that are in original condition.
    #[cfg_attr(feature = "__mutants", mutants::skip)]
    pub fn push(&self, val: rusqlite::Connection) {
        if let Some(a) = self.push_fast(val) {
            drop(a)
        }
    }

    // code optimized to hold lock for shortest time possible
    #[cfg_attr(feature = "__mutants", mutants::skip)]
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
