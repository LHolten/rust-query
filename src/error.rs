use std::{fmt::Debug, marker::PhantomData};

use crate::{Table, TableRow};

pub struct Conflict<T: Table> {
    _p: PhantomData<T>,
    msg: Box<dyn std::error::Error>,
}

impl<T: Table> Debug for Conflict<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Conflict")
            .field("table", &T::NAME)
            .field("msg", &self.msg)
            .finish()
    }
}

impl<T: Table> std::fmt::Display for Conflict<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Conflict in table `{}`", T::NAME)
    }
}

impl<T: Table> std::error::Error for Conflict<T> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.msg)
    }
}

impl<T: Table> std::fmt::Display for TableRow<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Row exists in table `{}`", T::NAME)
    }
}

impl<T: Table> std::error::Error for TableRow<T> {}
