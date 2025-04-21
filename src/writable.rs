use std::marker::PhantomData;

use ref_cast::{RefCastCustom, ref_cast_custom};

use crate::{
    Expr, IntoExpr, Table,
    alias::Field,
    ast::MySelect,
    value::{DynTypedExpr, NumTyp},
};

/// Defines a column update.
pub struct Update<'t, S, Typ> {
    inner: Box<dyn 't + Fn(Expr<'t, S, Typ>) -> Expr<'t, S, Typ>>,
}

impl<'t, S, Typ> Default for Update<'t, S, Typ> {
    fn default() -> Self {
        Self {
            inner: Box::new(|x| x),
        }
    }
}

impl<'t, S: 't, Typ: 't> Update<'t, S, Typ> {
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
    fn into_insert(self) -> <Self::T as Table>::Insert<'t>;
}

#[derive(RefCastCustom)]
#[repr(transparent)]
pub struct Reader<'t, S> {
    pub(crate) ast: MySelect,
    pub(crate) _p: PhantomData<S>,
    pub(crate) _p2: PhantomData<fn(&'t ()) -> &'t ()>,
}

impl<'t, S> Reader<'t, S> {
    #[ref_cast_custom]
    pub(crate) fn new(select: &MySelect) -> &Self;
}

impl<'t, S> Reader<'t, S> {
    pub fn col(&self, name: &'static str, val: impl IntoExpr<'t, S>) {
        self.col_erased(name, val.into_expr().inner.erase());
    }

    pub(crate) fn col_erased(&self, name: &'static str, val: DynTypedExpr) {
        let field = Field::Str(name);
        let expr = (val.0)(&self.ast.builder);
        self.ast.builder.select.push(Box::new((expr, field)))
    }
}
