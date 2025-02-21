use std::marker::PhantomData;

use crate::{
    alias::Field,
    ast::MySelect,
    value::{DynTypedExpr, Typed},
    IntoColumn, IntoDummy, Table,
};

/// this trait has to be implemented by the `schema` macro.
pub trait Insert<'t>: Update<'t> {}

/// this trait has to be implemented by the `schema` macro.
pub trait Update<'t> {
    type Schema;
    type T: Table<Schema = Self::Schema>;
    fn read(&self, f: Reader<'_, 't, Self::Schema>);

    type Conflict;
    fn get_conflict_unchecked(
        &self,
    ) -> impl IntoDummy<'t, 't, Self::Schema, Out = Option<Self::Conflict>>;
}

impl<'t, X: Update<'t>> Update<'t> for &X {
    type Schema = X::Schema;
    type T = X::T;

    fn read(&self, f: Reader<'_, 't, Self::Schema>) {
        X::read(self, f);
    }

    type Conflict = X::Conflict;

    fn get_conflict_unchecked(
        &self,
    ) -> impl IntoDummy<'t, 't, Self::Schema, Out = Option<Self::Conflict>> {
        X::get_conflict_unchecked(self)
    }
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
