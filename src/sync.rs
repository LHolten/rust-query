use std::{
    ops::Deref,
    process::abort,
    sync::{Arc, Condvar, Mutex},
};

use crate::{
    client::private_exec, exec::Execute, insert::Writable, value::MyTyp, Client, Covariant, HasId,
    Just,
};

#[derive(Clone)]
pub struct SharedClient {
    inner: Arc<Inner>,
}

pub struct Inner {
    client: Mutex<Client>,
    cvar: Condvar,
    updates: Mutex<bool>,
}

impl SharedClient {
    pub fn new(client: Client) -> Self {
        let inner = Arc::new(Inner {
            client: Mutex::new(client),
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
        private_exec(&self.client.lock().unwrap_or_else(|_| abort()).inner, f)
    }

    /// Please refer to [Client::get]
    pub fn get<'s, T: MyTyp>(&'s self, val: impl Covariant<'s, Typ = T>) -> T::Out<'s> {
        self.exec(|e| e.into_vec(move |row| row.get(val.clone().weaken())))
            .pop()
            .unwrap()
    }

    /// Please refer to [Client::try_insert]
    pub fn try_insert<'s, T: HasId>(
        &'s self,
        val: impl Writable<'s, T = T>,
    ) -> Option<Just<'s, T>> {
        let res = self
            .client
            .lock()
            .unwrap_or_else(|_| abort())
            .try_insert(val);
        *self.updates.lock().unwrap() = true;
        self.cvar.notify_all();
        res
    }
}
