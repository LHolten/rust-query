use std::{
    cell::Cell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use sea_query::{Iden, SqliteQueryBuilder};

use crate::{
    alias::Field,
    ast::MySelect,
    query::Query,
    value::{MyTyp, Value},
};

/// This is the top level query type and dereferences to [Query].
/// It has methods for turning queries into vectors and for inserting in the database.
pub struct Execute<'outer, 'inner> {
    pub(crate) phantom: PhantomData<&'outer ()>,
    pub(crate) q: Query<'inner>,
    pub(crate) conn: &'inner rusqlite::Transaction<'inner>,
}

impl<'outer, 'inner> Deref for Execute<'outer, 'inner> {
    type Target = Query<'inner>;

    fn deref(&self) -> &Self::Target {
        &self.q
    }
}

impl<'outer, 'inner> DerefMut for Execute<'outer, 'inner> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.q
    }
}

impl<'outer, 'inner> Execute<'outer, 'inner> {
    /// Turn a database query into a rust [Vec] of results.
    /// The callback is called exactly once for each row.
    /// The callback argument [Row] can be used to turn dummies into rust values.
    pub fn into_vec<F, T>(&self, mut f: F) -> Vec<T>
    where
        F: FnMut(Row<'_, 'outer, 'inner>) -> T,
    {
        let limit = u32::MAX;
        let mut select = self.ast.simple(0, limit);
        let sql = select.to_string(SqliteQueryBuilder);

        // eprintln!("{sql}");
        let conn = self.conn;
        let mut statement = conn.prepare(&sql).unwrap();
        let mut rows = statement.query([]).unwrap();

        let mut out = vec![];
        while let Some(row) = rows.next().unwrap() {
            let updated = Cell::new(false);
            let row = Row {
                offset: out.len(),
                limit,
                inner: PhantomData,
                inner2: PhantomData,
                row,
                ast: self.ast,
                conn,
                updated: &updated,
            };
            out.push(f(row));

            if updated.get() {
                // eprintln!("UPDATING!");

                select = self.ast.simple(out.len(), limit);
                let sql = select.to_string(SqliteQueryBuilder);
                // eprintln!("{sql}");

                drop(rows);
                statement = conn.prepare(&sql).unwrap();
                rows = statement.query([]).unwrap();
            }
        }
        out
    }
}

/// This is the type used by [Execute::into_vec] to allow turning dummies into rust values.
#[derive(Clone, Copy)]
pub struct Row<'x, 'outer, 'names> {
    pub(crate) offset: usize,
    pub(crate) limit: u32,
    pub(crate) inner: PhantomData<fn(&'names ()) -> &'names ()>,
    pub(crate) inner2: PhantomData<&'outer ()>,
    pub(crate) row: &'x rusqlite::Row<'x>,
    pub(crate) ast: &'x MySelect,
    pub(crate) conn: &'x rusqlite::Connection,
    pub(crate) updated: &'x Cell<bool>,
}

impl<'x, 'outer, 'names> Row<'x, 'outer, 'names> {
    /// Turn a dummy into a rust value.
    pub fn get<T: MyTyp>(&self, val: impl Value<'names, Typ = T>) -> T::Out<'outer> {
        let expr = val.build_expr(self.ast.builder());
        let Some((_, alias)) = self.ast.select.iter().find(|x| x.0 == expr) else {
            let alias = Field::new();

            self.ast.select.push(Box::new((expr, alias)));
            return self.requery(alias);
        };

        if self.updated.get() {
            // self.row is not up to date
            self.requery(*alias)
        } else {
            let idx = &*alias.to_string();
            self.row.get_unwrap(idx)
        }
    }

    fn requery<T: rusqlite::types::FromSql>(&self, alias: Field) -> T {
        let select = self.ast.simple(self.offset, self.limit);
        let sql = select.to_string(SqliteQueryBuilder);
        // eprintln!("REQUERY");
        // eprintln!("{sql}");
        let mut statement = self.conn.prepare(&sql).unwrap();
        let mut rows = statement.query([]).unwrap();
        self.updated.set(true);

        let idx = &*alias.to_string();
        rows.next().unwrap().unwrap().get_unwrap(idx)
    }
}
