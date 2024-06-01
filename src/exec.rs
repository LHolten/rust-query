use std::{
    cell::Cell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use sea_query::{Iden, SqliteQueryBuilder};

use crate::{
    ast::MySelect,
    query::Query,
    value::{Field, MyIdenT, Value},
};

/// This is the top level query type and dereferences to [Query].
/// It has methods for turning queries into vectors and for inserting in the database.
pub struct Exec<'outer, 'inner> {
    pub(crate) phantom: PhantomData<&'outer ()>,
    pub(crate) q: Query<'inner>,
}

impl<'outer, 'inner> Deref for Exec<'outer, 'inner> {
    type Target = Query<'inner>;

    fn deref(&self) -> &Self::Target {
        &self.q
    }
}

impl<'outer, 'inner> DerefMut for Exec<'outer, 'inner> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.q
    }
}

impl<'outer, 'inner> Exec<'outer, 'inner> {
    /// Turn a database query into a rust [Vec] of results.
    /// The callback is called exactly once for each row.
    /// The callback argument [Row] can be used to turn dummies into rust values.
    pub fn into_vec<F, T>(&self, limit: u32, mut f: F) -> Vec<T>
    where
        F: FnMut(Row<'_, 'inner>) -> T,
    {
        let mut select = self.ast.simple(0, limit);
        let sql = select.to_string(SqliteQueryBuilder);

        // eprintln!("{sql}");
        let conn = self.client;
        let mut statement = conn.prepare(&sql).unwrap();
        let mut rows = statement.query([]).unwrap();

        let mut out = vec![];
        while let Some(row) = rows.next().unwrap() {
            let updated = Cell::new(false);
            let row = Row {
                offset: out.len(),
                limit,
                inner: PhantomData,
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

/// This is the type used by [Exec::into_vec] to allow turning dummies into rust values.
#[derive(Clone, Copy)]
pub struct Row<'x, 'names> {
    pub(crate) offset: usize,
    pub(crate) limit: u32,
    pub(crate) inner: PhantomData<dyn Fn(&'names ())>,
    pub(crate) row: &'x rusqlite::Row<'x>,
    pub(crate) ast: &'x MySelect,
    pub(crate) conn: &'x rusqlite::Connection,
    pub(crate) updated: &'x Cell<bool>,
}

impl<'names> Row<'_, 'names> {
    /// Turn a dummy into a rust value.
    pub fn get<V: Value<'names>>(&self, val: V) -> V::Typ
    where
        V::Typ: MyIdenT + rusqlite::types::FromSql,
    {
        let expr = val.build_expr();
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

    fn requery<T: MyIdenT + rusqlite::types::FromSql>(&self, alias: Field) -> T {
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
