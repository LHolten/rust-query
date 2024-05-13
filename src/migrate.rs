use rusqlite::Connection;

pub trait TableMigration<'a> {
    type T;
}

pub trait Migration<From> {
    type S;
}

pub struct Migrator<'x, S> {
    schema: Option<S>,
    conn: &'x Connection,
}

impl<'a, S> Migrator<'a, S> {
    pub fn migrate<M: Migration<S>>(self, _f: impl FnOnce(&Self) -> M) -> Migrator<'a, M::S> {
        todo!()
    }

    pub fn check(self) -> S {
        todo!()
    }
}
