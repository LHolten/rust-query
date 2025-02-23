use std::marker::PhantomData;

use crate::{
    alias::Field,
    ast::MySelect,
    value::{DynTypedExpr, Typed},
    Column, IntoColumn, IntoDummy, Table,
};

pub struct Update<'t, S, Typ> {
    inner: Box<dyn 't + Fn(Column<'t, S, Typ>) -> Column<'t, S, Typ>>,
}

impl<'t, S, Typ> Default for Update<'t, S, Typ> {
    fn default() -> Self {
        Self {
            inner: Box::new(|x| x),
        }
    }
}

impl<'t, S: 't, Typ: 't> Update<'t, S, Typ> {
    pub fn set(val: impl IntoColumn<'t, S, Typ = Typ>) -> Self {
        let val = val.into_column();
        Self {
            inner: Box::new(move |_| val.clone()),
        }
    }

    #[doc(hidden)]
    pub fn apply(&self, val: impl IntoColumn<'t, S, Typ = Typ>) -> Column<'t, S, Typ> {
        (self.inner)(val.into_column())
    }
}

/// this trait has to be implemented by the `schema` macro.
pub trait TableInsert<'t>: TableConflict<'t> {
    fn read(&self, f: Reader<'_, 't, Self::Schema>);
    fn get_conflict_unchecked(
        &self,
    ) -> impl IntoDummy<'t, 't, Self::Schema, Out = Option<Self::Conflict>>;
}

/// this trait has to be implemented by the `schema` macro.
pub trait TableUpdate<'t>: TableConflict<'t> {
    fn read(&self, old: Column<'t, Self::Schema, Self::T>, f: Reader<'_, 't, Self::Schema>);
    fn get_conflict_unchecked(
        &self,
        old: Column<'t, Self::Schema, Self::T>,
    ) -> impl IntoDummy<'t, 't, Self::Schema, Out = Option<Self::Conflict>>;
}

pub trait TableConflict<'t> {
    type Schema;
    type T: Table<Schema = Self::Schema>;
    type Conflict;
}

pub struct Reader<'x, 't, S> {
    pub(crate) ast: &'x MySelect,
    pub(crate) _p: PhantomData<S>,
    pub(crate) _p2: PhantomData<fn(&'t ()) -> &'t ()>,
}

impl<'t, S> Reader<'_, 't, S> {
    pub fn col(&self, name: &'static str, val: impl IntoColumn<'t, S>) {
        let field = Field::Str(name);
        let val = val.into_column().inner;
        let expr = val.build_expr(self.ast.builder());
        self.ast.select.push(Box::new((expr, field)))
    }

    pub(crate) fn col_erased(&self, name: &'static str, val: DynTypedExpr) {
        let field = Field::Str(name);
        let expr = (val.0)(self.ast.builder());
        self.ast.select.push(Box::new((expr, field)))
    }
}
