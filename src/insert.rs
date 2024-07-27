use std::marker::PhantomData;

use sea_query::{Alias, InsertStatement, OnConflict, SqliteQueryBuilder};

use crate::{alias::Field, ast::MySelect, Client, Covariant, HasId, Just};

pub trait Writable<'a> {
    type T: HasId;
    fn read(self: Box<Self>, f: Reader<'_, 'a>);
}

pub struct Reader<'x, 'a> {
    pub(crate) _phantom: PhantomData<dyn Fn(&'a ()) -> &'a ()>,
    pub(crate) ast: &'x MySelect,
}

impl<'x, 'a> Reader<'x, 'a> {
    pub fn col(&self, name: &'static str, val: impl Covariant<'a>) {
        let field = Field::Str(name);
        let expr = val.build_expr(self.ast.builder());
        self.ast.select.push(Box::new((expr, field)))
    }
}

impl Client {
    pub fn try_insert<'a, T: HasId>(
        &'a self,
        val: impl Writable<'a, T = T>,
    ) -> Option<Just<'a, T>> {
        let ast = MySelect::default();

        let reader = Reader {
            _phantom: PhantomData,
            ast: &ast,
        };
        Writable::read(Box::new(val), reader);

        let select = ast.simple(0, u32::MAX);

        let mut insert = InsertStatement::new();
        // TODO: make this configurable
        insert.on_conflict(OnConflict::new().do_nothing().to_owned());
        let names = ast.select.iter().map(|(_field, name)| *name);
        insert.into_table(Alias::new(T::NAME));
        insert.columns(names);
        insert.select_from(select).unwrap();
        insert.returning_col(Alias::new(T::ID));

        let sql = insert.to_string(SqliteQueryBuilder);
        let id = self
            .inner
            .prepare(&sql)
            .unwrap()
            .query_map([], |row| row.get(T::ID))
            .unwrap()
            .next();
        id.map(|id| Just {
            _p: PhantomData,
            idx: id.unwrap(),
        })
    }
}
