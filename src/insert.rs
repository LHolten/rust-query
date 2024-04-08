use elsa::FrozenVec;
use sea_query::{Alias, Expr, InsertStatement, SimpleExpr, SqliteQueryBuilder};

use crate::{
    value::{Db, Field, MyIdenT},
    HasId, Query, Table,
};

pub trait TableWrite: Table {
    fn read<'a>(val: Self::Dummy<'a>, f: Reader<'a>);
}

pub struct Reader<'a> {
    parts: &'a FrozenVec<Box<(Field, SimpleExpr)>>,
}

impl<'a> Reader<'a> {
    pub fn col<T: MyIdenT>(&self, name: &'static str, val: Db<'a, T>) {
        let field = Field::Str(name);
        let expr = Expr::col(val.field).into();
        self.parts.push(Box::new((field, expr)))
    }
}

impl<'outer, 'inner> Query<'outer, 'inner> {
    pub fn insert<T: HasId + TableWrite>(&'outer self, val: T::Dummy<'outer>) {
        // TODO: fix this leak
        let last = Box::leak(Box::new(FrozenVec::new()));
        T::read(val, Reader { parts: last });

        let mut insert = InsertStatement::new();
        insert.into_table(Alias::new(T::NAME));

        let names = last.iter().map(|(name, _field)| *name);
        insert.columns(names);

        let select = self.joins.wrap(self.ast, 0, u32::MAX, last);

        insert.select_from(select).unwrap();
        let sql = insert.to_string(SqliteQueryBuilder);

        println!("{sql}");
        self.client.execute(&sql, []).unwrap();
    }
}
