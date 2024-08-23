use std::marker::PhantomData;

use rusqlite::Connection;
use sea_query::{Alias, InsertStatement, OnConflict, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;

use crate::{alias::Field, ast::MySelect, HasId, Free, Value};

/// this trait is not safe to implement
pub trait Writable {
    type T: HasId;
    fn read(self, f: Reader<'_>);
}

pub struct Reader<'x> {
    pub(crate) ast: &'x MySelect,
}

impl<'x> Reader<'x> {
    pub fn col(&self, name: &'static str, val: impl for<'a> Value<'a>) {
        let field = Field::Str(name);
        let expr = val.build_expr(self.ast.builder());
        self.ast.select.push(Box::new((expr, field)))
    }
}

pub(crate) fn private_try_insert<'a, T: HasId>(
    conn: &Connection,
    val: impl Writable<T = T>,
) -> Option<Free<'a, T>> {
    let ast = MySelect::default();

    let reader = Reader { ast: &ast };
    Writable::read(val, reader);

    let select = ast.simple();

    let mut insert = InsertStatement::new();
    // TODO: make this configurable
    insert.on_conflict(OnConflict::new().do_nothing().to_owned());
    let names = ast.select.iter().map(|(_field, name)| *name);
    insert.into_table(Alias::new(T::NAME));
    insert.columns(names);
    insert.select_from(select).unwrap();
    insert.returning_col(Alias::new(T::ID));

    let (sql, values) = insert.build_rusqlite(SqliteQueryBuilder);

    let mut statement = conn.prepare(&sql).unwrap();
    let id = statement
        .query_map(&*values.as_params(), |row| row.get(T::ID))
        .unwrap()
        .next();
    id.map(|id| Free {
        _p: PhantomData,
        idx: id.unwrap(),
    })
}
