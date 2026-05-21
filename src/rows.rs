use std::{marker::PhantomData, rc::Rc};

use crate::{
    CustomJoin, Expr, IntoExpr, Table, TableRow,
    joinable::IntoJoinable,
    lower,
    private::Joinable,
    value::{DbTyp, EqTyp, MyTableRef},
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
    pub(crate) ast: Rc<lower::Select>,
}

impl<'inner, S> Rows<'inner, S> {
    /// Join a table, this is like a super simple [Iterator::flat_map] but for queries.
    ///
    /// After this operation [Rows] has rows for the combinations of each original row with each row of the table.
    /// (Also called the "Carthesian product")
    /// The expression that is returned refers to the joined table.
    ///
    /// The parameter must be a table name from the schema like `v0::User`.
    /// This table can be filtered by `#[index]`: `rows.join(v0::User.score(100))`.
    ///
    /// See also [Self::filter_some] if you want to join a table that is filtered by `#[unique]`.
    pub fn join<T: DbTyp>(
        &mut self,
        j: impl IntoJoinable<'inner, S, Typ = T>,
    ) -> Expr<'inner, S, T> {
        let joinable = j.into_joinable();

        let table_idx = self.ast.tables.len();
        let join = self.ast.join(joinable.table);
        for (name, val) in joinable.conds {
            // it is fine to directly use the alias here because the filter is in the same scope as the join
            let expr = Rc::new(lower::Expr::RowIndex(lower::RowLike::Join(join), name));
            self.filter(Expr::adhoc(lower::Expr::Infix(expr, "=", val)));
        }

        let table_idx = MyTableRef {
            scope_rc: self.ast.scope_rc.clone(),
            idx: table_idx,
            table_name: joinable.table,
        };

        Expr::adhoc(lower::Expr::RowIndex(
            lower::RowLike::Join(join),
            joinable.table.main_column(),
        ))
    }

    #[doc(hidden)]
    pub fn join_private<T: Table<Schema = S>>(&mut self) -> Expr<'inner, S, TableRow<T>> {
        self.join(Joinable::table())
    }

    pub(crate) fn join_custom<T: CustomJoin<Schema = S>>(
        &mut self,
        t: T,
    ) -> Expr<'inner, S, TableRow<T>> {
        self.join(Joinable::new(t.name()))
    }

    pub(crate) fn join_tmp<T: Table<Schema = S>>(
        &mut self,
        tmp: lower::TmpTable,
    ) -> Expr<'inner, S, TableRow<T>> {
        self.join(Joinable::new(lower::JoinableTable::Tmp(tmp)))
    }

    /// Filter rows based on an expression.
    pub fn filter(&mut self, prop: impl IntoExpr<'inner, S, Typ = bool>) {
        Rc::make_mut(&mut self.ast).filter(prop.into_expr().inner);
    }

    /// Filter out rows where this expression is [None].
    ///
    /// Returns a new expression with the unwrapped type.
    pub fn filter_some<Typ: EqTyp>(
        &mut self,
        val: impl IntoExpr<'inner, S, Typ = Option<Typ>>,
    ) -> Expr<'inner, S, Typ> {
        let val = val.into_expr();
        self.ast.filter(val.inner.clone());

        // we already removed all rows with null, so this is ok.
        Expr::new(val.inner)
    }
}
