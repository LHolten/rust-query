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
// #[must_use]
// pub struct Col<'a, T: MyIdenT>(Db<'a, T>);

// impl<'a, T: MyIdenT> Value<'a> for Col<'a, T> {
//     type Typ = T;

//     fn build_expr(&self) -> sea_query::SimpleExpr {
//         self.0.build_expr()
//     }
// }

// pub struct MutTable<'a> {
//     p: PhantomData<dyn Fn(&'a ()) -> &'a ()>,
// }

// impl<'a> MutTable<'a> {
//     // pub fn initial_unique<Typ: MyIdenT>(&self, vals: ) {
//     //     todo!()
//     // }

//     pub fn column<T: MyIdenT>(&mut self, name: &'static str) -> Col<'a, T> {
//         todo!()
//     }

//     pub fn new_column<O, F>(&'a self, name: &'static str, f: F) -> Col<'a, O::Typ>
//     where
//         F: FnMut(Row<'_, 'a>) -> O,
//         O: Value<'a>,
//     {
//         todo!()
//     }

//     pub fn drop_column<T: MyIdenT>(&'a self, col: Col<'a, T>) {
//         todo!()
//     }
// }

// pub const fn new_table<T: Table>() -> T {
//     todo!()
// }

pub trait Schema: Sized {
    const SQL: &'static str;

    type Prev;
    type Migration;

    fn new(prev: Self::Prev, m: Self::Migration) -> Self;
    // fn new() -> Self;

    fn migrate<S: Schema<Prev = Self>>(self, m: S::Migration) {
        todo!()
    }
}

// pub trait MigrateTable: Table {
//     /// Writable is boxed, because otherwise type inference fails
//     fn migrate<F, T>(&self, f: F) -> T
//     where
//         F: for<'x> FnOnce(&'x mut MutTable<'x>, Db<'x, Self>) -> Box<dyn Writable<'x, T = T> + 'x>,
//     {
//         todo!()
//     }
// }

// impl<T: Table> MigrateTable for T {}

// use rusqlite_migration::M;

// pub fn migrate_table<F>(name: &'static str, f: F) -> M
// where
//     F: for<'x> FnOnce(&'x mut MutTable<'x>),
// {
//     todo!()
// }

// #[test]
// fn feature() {
//     migrate_table("Artist", |t| {
//         t.new_column("second name", |_row| 1);
//     });
// }

// pub trait Migration {
//     type Prev;
//     type Next;
// }
