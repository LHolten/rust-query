use std::marker::PhantomData;

use crate::{
    Expr, IntoExpr, Table,
    value::{DynTypedExpr, MyTyp, NumTyp},
};

/// Defines a column update.
pub struct Update<S, Typ: MyTyp> {
    inner: Box<dyn Fn(Expr<'static, S, Typ>) -> Expr<'static, S, Typ>>,
}

impl<S, Typ: MyTyp> Default for Update<S, Typ> {
    fn default() -> Self {
        Self {
            inner: Box::new(|x| x),
        }
    }
}

impl<S: 'static, Typ: MyTyp> Update<S, Typ> {
    /// Set the new value of the column.
    pub fn set(val: impl IntoExpr<'static, S, Typ = Typ>) -> Self {
        let val = val.into_expr();
        Self {
            inner: Box::new(move |_| val.clone()),
        }
    }

    #[doc(hidden)]
    pub fn apply(&self, val: impl IntoExpr<'static, S, Typ = Typ>) -> Expr<'static, S, Typ> {
        (self.inner)(val.into_expr())
    }
}

impl<S: 'static, Typ: NumTyp> Update<S, Typ> {
    /// Update the column value to the old value plus some new value.
    pub fn add(val: impl IntoExpr<'static, S, Typ = Typ>) -> Self {
        let val = val.into_expr();
        Self {
            inner: Box::new(move |old| old.add(&val)),
        }
    }
}

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
        self.col_erased(name, val.into_expr().inner.erase());
    }

    pub(crate) fn col_erased(&mut self, name: &'static str, val: DynTypedExpr) {
        self.builder.push((name, val));
    }
}
