use std::marker::PhantomData;

use crate::{
    alias::Field,
    ast::MySelect,
    value::{DynTypedExpr, NumTyp, Typed},
    Expr, IntoColumn, IntoDummy, Table,
};

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
    pub fn set(val: impl IntoColumn<'t, S, Typ = Typ>) -> Self {
        let val = val.into_column();
        Self {
            inner: Box::new(move |_| val.clone()),
        }
    }

    #[doc(hidden)]
    pub fn apply(&self, val: impl IntoColumn<'t, S, Typ = Typ>) -> Expr<'t, S, Typ> {
        (self.inner)(val.into_column())
    }
}

impl<'t, S: 't, Typ: NumTyp> Update<'t, S, Typ> {
    pub fn add(val: impl IntoColumn<'t, S, Typ = Typ>) -> Self {
        let val = val.into_column();
        Self {
            inner: Box::new(move |old| old.add(&val)),
        }
    }
}

/// this trait has to be implemented by the `schema` macro.
pub trait TableInsert<'t> {
    type Schema;
    type Conflict;
    type T: Table<Schema = Self::Schema, Conflict<'t> = Self::Conflict>;

    fn read(&self, f: Reader<'_, 't, Self::Schema>);
    fn get_conflict_unchecked(
        &self,
    ) -> impl IntoDummy<'t, 't, Self::Schema, Out = Option<Self::Conflict>>;
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
