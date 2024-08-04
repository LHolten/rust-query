use std::marker::PhantomData;

use sea_query::Expr;

use crate::{
    alias::MyAlias,
    ast::{add_table, MySelect, Source},
    db::Db,
    group::Aggregate,
    value::{Assume, Value},
    Table,
};

/// This is the base type for other query types like [crate::args::Aggregate] and [crate::args::Execute].
/// It contains most query functionality like joining tables and doing sub-queries.
///
/// [Query] mutability is only about the number of rows.
/// Adding columns to a [Query] does not require mutation.
pub struct Query<'inner> {
    // we might store 'inner
    pub(crate) phantom: PhantomData<fn(&'inner ()) -> &'inner ()>,
    pub(crate) ast: &'inner mut MySelect,
}

impl<'inner> Query<'inner> {
    /// Join a table, this is like [Iterator::flat_map] but for queries.
    pub fn join<T: Table>(&mut self, t: &T) -> Db<'inner, T> {
        let table = add_table(&mut self.ast.tables, t.name());
        Db::new(table)
    }

    /// Join a vector of values.
    // pub fn vec<V: Value<'inner>>(&mut self, vec: Vec<V>) -> Db<'inner, V::Typ> {
    //     todo!()
    // }

    /// Perform a sub-query that returns a single result for each of the current rows.
    pub fn query<F, R>(&self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Aggregate<'inner, 'a>) -> R,
    {
        let mut ast = MySelect::default();
        let mut conds = Vec::new();
        let inner = Query {
            phantom: PhantomData,
            ast: &mut ast,
        };
        let table = MyAlias::new();
        let mut group = Aggregate {
            outer_ast: self.ast,
            conds: &mut conds,
            query: inner,
            table,
            phantom2: PhantomData,
        };
        let res = f(&mut group);

        let source = Source {
            conds,
            kind: crate::ast::SourceKind::Aggregate(ast),
        };
        self.ast.extra.get_or_init(source, || table);
        res
    }

    /// Filter rows based on a column.
    pub fn filter(&mut self, prop: impl Value<'inner>) {
        self.ast
            .filters
            .push(Box::new(prop.build_expr(self.ast.builder())));
    }

    /// Filter out rows where this column is [None].
    pub fn filter_some<T, V>(&mut self, val: V) -> Assume<V>
    where
        V: Value<'inner, Typ = Option<T>>,
    {
        self.ast.filters.push(
            Box::new(Expr::expr(val.build_expr(self.ast.builder())))
                .is_not_null()
                .into(),
        );
        Assume(val)
    }
}
