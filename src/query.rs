use std::marker::PhantomData;

use sea_query::Expr;

use crate::{
    alias::{Field, MyAlias},
    ast::{add_table, MySelect, Source},
    db::{Db, DbCol},
    group::Aggregate,
    value::{Assume, Value},
    HasId, Table,
};

/// This is the base type for other query types like [crate::args::Aggregate] and [crate::args::Execute].
/// It contains most query functionality like joining tables and doing sub-queries.
///
/// [Query] mutability is only about the number of rows.
/// Adding columns to a [Query] does not require mutation.
/// And it is impossible to remove a column from a [Query].
///
/// [Db] borrows the values in a table column immutably.
/// Combining this with a [crate::args::Row] gives the actual value
///
/// Table mutability is about both number of rows and values.
/// This means that even inserting in a table requires mutable access.
pub struct Query<'inner> {
    // we might store 'inner
    pub(crate) phantom: PhantomData<fn(&'inner ()) -> &'inner ()>,
    pub(crate) ast: &'inner mut MySelect,
}

impl<'inner> Query<'inner> {
    /// Join a table, this is like [Iterator::flat_map] but for queries.
    pub fn table<T: HasId>(&mut self, t: &T) -> DbCol<'inner, T> {
        let table = add_table(&mut self.ast.tables, t.name());
        DbCol::db(table, Field::Str(T::ID))
    }

    /// Join a table that has no integer primary key.
    /// Refer to [Query::table] for more information about joining tables.
    pub fn flat_table<T: Table>(&mut self, t: T) -> Db<'inner, T> {
        let table = add_table(&mut self.ast.tables, t.name());
        Db {
            table,
            _p: PhantomData,
        }
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
        let inner = Query {
            phantom: PhantomData,
            ast: &mut ast,
        };
        let table = MyAlias::new();
        let mut group = Aggregate {
            outer_ast: self.ast,
            query: inner,
            table,
            phantom2: PhantomData,
        };
        let res = f(&mut group);

        self.ast.extra.get_or_init(Source::Aggregate(ast), || table);
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
