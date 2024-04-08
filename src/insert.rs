use elsa::FrozenVec;
use sea_query::{Alias, InsertStatement, OnConflict, SimpleExpr, SqliteQueryBuilder};

use crate::{
    value::{Field, Value},
    Exec, HasId,
};

pub trait Writable<'a> {
    type T: HasId;
    fn read(self, f: Reader<'a>);
}

pub struct Reader<'a> {
    parts: &'a FrozenVec<Box<(Field, SimpleExpr)>>,
}

impl<'a> Reader<'a> {
    pub fn col(&self, name: &'static str, val: impl Value<'a>) {
        let field = Field::Str(name);
        let expr = val.build_expr();
        self.parts.push(Box::new((field, expr)))
    }
}

impl<'outer, 'inner> Exec<'outer, 'inner> {
    pub fn insert<V: Writable<'outer>>(&'outer self, val: V) {
        // TODO: fix this leak
        let last = Box::leak(Box::new(FrozenVec::new()));
        V::read(val, Reader { parts: last });

        let mut insert = InsertStatement::new();
        // TODO: make this configurable
        insert.on_conflict(OnConflict::new().do_nothing().to_owned());
        insert.into_table(Alias::new(V::T::NAME));

        let names = last.iter().map(|(name, _field)| *name);
        insert.columns(names);

        let select = self.joins.wrap(self.ast, 0, u32::MAX, last);

        insert.select_from(select).unwrap();
        let sql = insert.to_string(SqliteQueryBuilder);

        println!("{sql}");
        self.client.execute(&sql, []).unwrap();
    }
}

// let parts = Box::leak(Box::new(FrozenVec::new()));
// V::read(val, Reader { parts });

// let mut insert = InsertStatement::new();
// // TODO: make this configurable
// insert.on_conflict(OnConflict::new().do_nothing().to_owned());
// insert.into_table(Alias::new(V::T::NAME));

// let names = parts.iter().map(|(name, _expr)| *name);
// insert.columns(names);

// let mut select = self.ast.simple(0, u32::MAX);
// select.clear_selects();

// let values = parts.iter().map(|(_name, expr)| expr.clone());
// select.exprs(values);
