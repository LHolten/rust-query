use std::marker::PhantomData;

use crate::{IntoExpr, Table, value::DynTypedExpr};

/// this trait has to be implemented by the `schema` macro.
pub trait TableInsert {
    type T: Table;
    fn into_insert(self) -> <Self::T as Table>::Insert;
}

pub struct Reader<S> {
    pub(crate) builder: Vec<(&'static str, DynTypedExpr)>,
    pub(crate) _p: PhantomData<S>,
}

impl<S> Default for Reader<S> {
    fn default() -> Self {
        Self {
            builder: Default::default(),
            _p: Default::default(),
        }
    }
}

impl<S> Reader<S> {
    pub fn col(&mut self, name: &'static str, val: impl IntoExpr<'static, S>) {
        self.col_erased(name, DynTypedExpr::erase(val));
    }

    pub(crate) fn col_erased(&mut self, name: &'static str, val: DynTypedExpr) {
        self.builder.push((name, val));
    }
}
