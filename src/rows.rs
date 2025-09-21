use std::{marker::PhantomData, rc::Rc};

use sea_query::IntoIden;

use crate::{
    Expr, Table,
    alias::{JoinableTable, TmpTable},
    ast::MySelect,
    db::Join,
    value::{IntoExpr, MyTableRef, MyTyp, Typed},
};

/// [Rows] keeps track of all rows in the current query.
///
/// This is the base type for other query types like [crate::args::Aggregate] and [crate::args::Query].
/// It contains basic query functionality like joining tables and filters.
///
/// [Rows] mutability is only about which rows are included.
/// Adding new columns does not require mutating [Rows].
pub struct Rows<'inner, S> {
    // we might store 'inner
    pub(crate) phantom: PhantomData<fn(&'inner ()) -> &'inner ()>,
    pub(crate) _p: PhantomData<S>,
    pub(crate) ast: Rc<MySelect>,
}

impl<'inner, S> Rows<'inner, S> {
    /// Join a table, this is like a super simple [Iterator::flat_map] but for queries.
    ///
    /// After this operation [Rows] has rows for the combinations of each original row with each row of the table.
    /// (Also called the "Carthesian product")
    ///
    /// The expression that is returned refers to the joined table.
    pub fn join<T: Table<Schema = S>>(&mut self, _: T) -> Expr<'inner, S, T> {
        self.join_inner(JoinableTable::Normal(T::NAME.into()))
    }

    pub(crate) fn join_custom<T: Table<Schema = S>>(&mut self, t: T) -> Expr<'inner, S, T> {
        self.join_inner(t.name())
    }

    pub(crate) fn join_tmp<T: Table<Schema = S>>(&mut self, tmp: TmpTable) -> Expr<'inner, S, T> {
        let tmp_string = tmp.into_iden();
        self.join_inner(JoinableTable::Normal(tmp_string))
    }

    fn join_inner<T: Table<Schema = S>>(&mut self, name: JoinableTable) -> Expr<'inner, S, T> {
        let table_idx = self.ast.tables.len();
        Rc::make_mut(&mut self.ast).tables.push(name);
        Expr::new(Join::new(MyTableRef {
            scope_rc: self.ast.scope_rc.clone(),
            idx: table_idx,
        }))
    }

    // Join a vector of values.
    // pub fn vec<V: IntoExpr<'inner>>(&mut self, vec: Vec<V>) -> Join<'inner, V::Typ> {
    //     todo!()
    // }

    /// Filter rows based on an expression.
    pub fn filter(&mut self, prop: impl IntoExpr<'inner, S, Typ = bool>) {
        let prop = prop.into_expr().inner.erase();
        Rc::make_mut(&mut self.ast).filters.push(prop);
    }

    /// Filter out rows where this expression is [None].
    ///
    /// Returns a new expression with the unwrapped type.
    pub fn filter_some<Typ: MyTyp>(
        &mut self,
        val: impl IntoExpr<'inner, S, Typ = Option<Typ>>,
    ) -> Expr<'inner, S, Typ> {
        let val = val.into_expr();
        Rc::make_mut(&mut self.ast)
            .filters
            .push(val.is_some().inner.erase());

        Expr::adhoc(move |b| val.inner.build_expr(b))
    }
}
