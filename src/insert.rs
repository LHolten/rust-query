use std::marker::PhantomData;

use rusqlite::{Connection, ErrorCode};
use sea_query::{Alias, InsertStatement, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;

use crate::{alias::Field, ast::MySelect, IntoColumn, Table, TableRow};

/// this trait is not safe to implement
pub trait Writable<'t> {
    type T: Table;
    fn read(self, f: Reader<'_, 't, <Self::T as Table>::Schema>);
}

pub struct Reader<'x, 't, S> {
    pub(crate) ast: &'x MySelect,
    pub(crate) _p: PhantomData<S>,
    pub(crate) _p2: PhantomData<fn(&'t ()) -> &'t ()>,
}

impl<'t, S> Reader<'_, 't, S> {
    pub fn col(&self, name: &'static str, val: impl IntoColumn<'t, S>) {
        let field = Field::Str(name);
        let expr = val.build_expr(self.ast.builder());
        self.ast.select.push(Box::new((expr, field)))
    }
}

pub(crate) fn private_try_insert<'t, T: Table>(
    conn: &Connection,
    val: impl Writable<'t, T = T>,
) -> Option<TableRow<'t, T>> {
    let ast = MySelect::default();

    let reader = Reader {
        ast: &ast,
        _p: PhantomData,
        _p2: PhantomData,
    };
    Writable::read(val, reader);

    let select = ast.simple();

    let mut insert = InsertStatement::new();
    let names = ast.select.iter().map(|(_field, name)| *name);
    insert.into_table(Alias::new(T::NAME));
    insert.columns(names);
    insert.select_from(select).unwrap();
    insert.returning_col(Alias::new(T::ID));

    let (sql, values) = insert.build_rusqlite(SqliteQueryBuilder);

    let mut statement = conn.prepare_cached(&sql).unwrap();
    let mut res = statement
        .query_map(&*values.as_params(), |row| row.get(T::ID))
        .unwrap();

    match res.next().unwrap() {
        Ok(id) => Some(id),
        Err(rusqlite::Error::SqliteFailure(kind, Some(_val)))
            if kind.code == ErrorCode::ConstraintViolation =>
        {
            // val looks like "UNIQUE constraint failed: playlist_track.playlist, playlist_track.track"
            None
        }
        Err(err) => Err(err).unwrap(),
    }
}
