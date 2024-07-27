use std::{
    marker::PhantomData,
    ops::Deref,
    sync::{Arc, Condvar, Mutex},
};

use crate::{
    client::{private_exec, Weaken},
    exec::Execute,
    insert::Writable,
    value::MyTyp,
    Client, Covariant, HasId, Just,
};

#[derive(Clone)]
pub struct SharedClient {
    inner: Arc<Inner>,
}

pub struct Inner {
    conn: Mutex<Client>,
    cvar: Condvar,
    updates: Mutex<bool>,
}

impl SharedClient {
    pub fn new(conn: Client) -> Self {
        let inner = Arc::new(Inner {
            conn: Mutex::new(conn),
            cvar: Condvar::new(),
            updates: Mutex::new(true),
        });
        Self {
            inner: inner.clone(),
        }
    }
}

impl Deref for SharedClient {
    type Target = Inner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Inner {
    /// Wait for any changes to the database.
    pub fn wait(&self) {
        let updates = self.updates.lock().unwrap();
        *self.cvar.wait_while(updates, |&mut x| !x).unwrap() = false;
    }

    /// Please refer to [Client::exec]
    pub fn exec<'s, F, R>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Execute<'s, 'a>) -> R,
    {
        private_exec(&self.conn.lock().unwrap().inner, f)
    }

    /// Please refer to [Client::get]
    pub fn get<'s, T: MyTyp>(&'s self, val: impl Covariant<'s, Typ = T>) -> T::Out<'s> {
        let weak = Weaken {
            inner: val,
            _p: PhantomData,
        };
        self.exec(|e| e.into_vec(move |row| row.get(weak.clone())))
            .pop()
            .unwrap()
    }

    /// Please refer to [Client::try_insert]
    pub fn try_insert<'s, T: HasId>(
        &'s self,
        val: impl Writable<'s, T = T>,
    ) -> Option<Just<'s, T>> {
        self.conn.lock().unwrap().try_insert(val)
    }
}
