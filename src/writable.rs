use std::marker::PhantomData;

use crate::{
    Expr, IntoExpr, Table,
    value::{DynTypedExpr, MyTyp, NumTyp},
};

/// Defines a column update.
pub struct Update<'t, S, Typ: MyTyp> {
    inner: Box<dyn 't + Fn(Expr<'t, S, Typ>) -> Expr<'t, S, Typ>>,
}

impl<S, Typ: MyTyp> Default for Update<'_, S, Typ> {
    fn default() -> Self {
        Self {
            inner: Box::new(|x| x),
        }
    }
}

impl<'t, S: 't, Typ: MyTyp> Update<'t, S, Typ> {
    /// Set the new value of the column.
    pub fn set(val: impl IntoExpr<'t, S, Typ = Typ>) -> Self {
        let val = val.into_expr();
        Self {
            inner: Box::new(move |_| val.clone()),
        }
    }

    #[doc(hidden)]
    pub fn apply(&self, val: impl IntoExpr<'t, S, Typ = Typ>) -> Expr<'t, S, Typ> {
        (self.inner)(val.into_expr())
    }
}

impl<'t, S: 't, Typ: NumTyp> Update<'t, S, Typ> {
    /// Update the column value to the old value plus some new value.
    pub fn add(val: impl IntoExpr<'t, S, Typ = Typ>) -> Self {
        let val = val.into_expr();
        Self {
            inner: Box::new(move |old| old.add(&val)),
        }
    }
}

/// this trait has to be implemented by the `schema` macro.
pub trait TableInsert<'t> {
    type T: Table;
    fn into_insert(self) -> <Self::T as Table>::Insert;
}

pub struct Reader<'t, S> {
    pub(crate) builder: Vec<(&'static str, DynTypedExpr)>,
    pub(crate) _p: PhantomData<S>,
    pub(crate) _p2: PhantomData<fn(&'t ()) -> &'t ()>,
}

impl<'t, S> Default for Reader<'t, S> {
    fn default() -> Self {
        Self {
            builder: Default::default(),
            _p: Default::default(),
            _p2: Default::default(),
        }
    }
}

impl<'t, S> Reader<'t, S> {
    pub fn col(&mut self, name: &'static str, val: impl IntoExpr<'t, S>) {
        self.col_erased(name, val.into_expr().inner.erase());
    }

    pub(crate) fn col_erased(&mut self, name: &'static str, val: DynTypedExpr) {
        self.builder.push((name, val));
    }
}
