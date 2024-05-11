use std::{marker::PhantomData, ops::Deref};

use rusqlite::Connection;

use crate::{
    insert::Writable,
    pragma, schema,
    value::{Db, MyIdenT, Value},
    HasId, Query, Row, Table,
};

// struct Schema<T> {
//     inner: T,
// }

// pub fn load<T: Schema>() -> T {
//     todo!()
// }

// fn new_table<T: HasId, F>(f: F) -> T
// where
//     F: for<'a> FnOnce(&'a mut Query<'a>) -> T::Dummy<'a>,
// {
//     todo!()
// }

// pub trait Migration: Table {
//     fn migrate<'a>(table: Self::Dummy<'a>, row: Row<'_, 'a>) -> impl Writable<'a>;
// }

// pub trait FullMigration: Schema {
//     fn migrate
// }

/// Col does not implement Deref, this prevents implicit joins
#[must_use]
pub struct Col<'a, T: MyIdenT>(Db<'a, T>);

impl<'a, T: MyIdenT> Value<'a> for Col<'a, T> {
    type Typ = T;

    fn build_expr(&self) -> sea_query::SimpleExpr {
        self.0.build_expr()
    }
}

pub struct MutTable<'a> {
    p: PhantomData<dyn Fn(&'a ()) -> &'a ()>,
}

impl<'a> MutTable<'a> {
    pub fn initial_column<Typ: MyIdenT>(&mut self) -> Col<'a, Typ> {
        todo!()
    }

    // pub fn initial_unique<Typ: MyIdenT>(&self, vals: ) {
    //     todo!()
    // }

    pub fn new_column<O, F>(&'a self, f: F) -> Col<'a, O::Typ>
    where
        F: FnMut(Row<'_, 'a>) -> O,
        O: Value<'a>,
    {
        todo!()
    }

    pub fn drop_column<T: MyIdenT>(&'a self, col: Col<'a, T>) {
        todo!()
    }

    pub fn version(&'a self, version: usize) {
        todo!()
    }
}

/// Result is boxed because otherwise type inference fails
pub fn migrate_table<F, T>(f: F) -> T
where
    F: for<'x> FnOnce(&'x mut MutTable<'x>) -> Box<dyn Writable<'x, T = T> + 'x>,
{
    todo!()
}

// trait Schema {
//     const SQL: &'static str;
//     // fn new() -> Self;
// }
