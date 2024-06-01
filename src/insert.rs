use std::marker::PhantomData;

use sea_query::{Alias, InsertStatement, OnConflict, SqliteQueryBuilder};

use crate::{
    ast::MySelect,
    exec::Exec,
    value::{Field, Value},
    HasId,
};

pub trait Writable<'a> {
    type T: HasId;
    fn read(self: Box<Self>, f: Reader<'_, 'a>);
}

pub struct Reader<'x, 'a> {
    pub(crate) _phantom: PhantomData<dyn Fn(&'a ()) -> &'a ()>,
    pub(crate) ast: &'x MySelect,
}

impl<'x, 'a> Reader<'x, 'a> {
    pub fn col(&self, name: &'static str, val: impl Value<'a>) {
        let field = Field::Str(name);
        let expr = val.build_expr();
        self.ast.select.push(Box::new((expr, field)))
    }
}

impl<'outer, 'inner> Exec<'outer, 'inner> {
    /// Insert a new row for every row in the query.
    pub fn insert<V: Writable<'inner>>(&'inner mut self, val: V) {
        // insert can be used only once, and can not be used with select or group
        // this means that `self.ast.select` will contain exactly our columns
        // TODO: instead of directly inserting, might be better to make new names
        // and assign those (also i think INSERT doesn't care about the names)
        let reader = Reader {
            _phantom: PhantomData,
            ast: self.ast,
        };
        V::read(Box::new(val), reader);

        let mut insert = InsertStatement::new();
        // TODO: make this configurable
        insert.on_conflict(OnConflict::new().do_nothing().to_owned());
        insert.into_table(Alias::new(V::T::NAME));

        let names = self.ast.select.iter().map(|(_field, name)| *name);
        insert.columns(names);

        let select = self.ast.simple(0, u32::MAX);

        insert.select_from(select).unwrap();
        let sql = insert.to_string(SqliteQueryBuilder);

        self.client.execute(&sql, []).unwrap();
    }
}
