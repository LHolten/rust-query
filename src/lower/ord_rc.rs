use std::{
    ops::{Deref, DerefMut},
    rc::Rc,
};

#[derive(Debug)]
pub struct OrdRc<T: ?Sized>(pub Rc<T>);

impl OrdRc<rusqlite::types::Value> {
    pub fn new<T: Into<rusqlite::types::Value>>(val: T) -> Self {
        Self(Rc::new(val.into()))
    }
}

impl rusqlite::ToSql for OrdRc<rusqlite::types::Value> {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        self.0.as_ref().to_sql()
    }
}

impl<T: ?Sized> Clone for OrdRc<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: ?Sized> Deref for OrdRc<T> {
    type Target = Rc<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: ?Sized> DerefMut for OrdRc<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: ?Sized> Eq for OrdRc<T> {}

impl<T: ?Sized> Ord for OrdRc<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        Rc::as_ptr(&self.0)
            .cast::<()>()
            .cmp(&Rc::as_ptr(&other.0).cast())
    }
}

impl<T: ?Sized> PartialEq for OrdRc<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::as_ptr(&self.0)
            .cast::<()>()
            .eq(&Rc::as_ptr(&other.0).cast())
    }
}

impl<T: ?Sized> PartialOrd for OrdRc<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Rc::as_ptr(&self.0)
            .cast::<()>()
            .partial_cmp(&Rc::as_ptr(&other.0).cast())
    }
}
