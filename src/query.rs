use std::marker::PhantomData;

use elsa::FrozenVec;
use sea_query::Expr;

use crate::{
    ast::{add_table, Joins, MySelect, Source},
    group::Aggregate,
    value::{Db, Field, FkInfo, MyAlias, MyIdenT, Value},
    Builder, HasId, Table,
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
    pub(crate) phantom: PhantomData<dyn Fn(&'inner ()) -> &'inner ()>,
    pub(crate) ast: &'inner MySelect,
    pub(crate) client: &'inner rusqlite::Connection,
}

impl<'inner> Query<'inner> {
    /// Join a table, this is like [Iterator::flat_map] but for queries.
    pub fn table<T: HasId>(&mut self, t: &T) -> Db<'inner, T> {
        let joins = add_table(&self.ast.sources, t.name());
        FkInfo::joined(joins, Field::Str(T::ID))
    }

    /// Join a table that has no integer primary key.
    /// Refer to [Query::table] for more information about joining tables.
    pub fn flat_table<T: Table>(&mut self, t: T) -> T::Dummy<'inner> {
        let joins = add_table(&self.ast.sources, t.name());
        T::build(Builder::new(joins))
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
        let joins = Joins {
            table: MyAlias::new(),
            joined: FrozenVec::new(),
        };
        let source = Source::Select(MySelect::default(), joins);
        let source = self.ast.sources.push_get(Box::new(source));
        let Source::Select(ast, joins) = source else {
            unreachable!()
        };
        ast.group.set(true);
        let inner = Query {
            phantom: PhantomData,
            ast,
            client: self.client,
        };
        let mut group = Aggregate {
            query: inner,
            joins,
            phantom2: PhantomData,
        };
        f(&mut group)
    }

    /// Filter rows based on a column.
    pub fn filter(&mut self, prop: impl Value<'inner>) {
        self.ast.filters.push(Box::new(prop.build_expr()));
    }

    /// Filter out rows where this column is [None].
    pub fn filter_some<T: MyIdenT>(&mut self, val: &Db<'inner, Option<T>>) -> Db<'inner, T> {
        self.ast
            .filters
            .push(Box::new(Expr::expr(val.build_expr())).is_not_null().into());
        Db {
            info: val.info.clone(),
            field: val.field,
        }
    }
}
