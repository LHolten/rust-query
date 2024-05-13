pub trait TableMigration<'a> {
    type T;
}

pub trait Migration<From> {
    type S: Schema;
}

pub trait Schema: Sized {
    // const SQL: &'static str;

    fn migrate<M: Migration<Self>>(self, _f: impl FnOnce(&Self) -> M) -> M::S {
        todo!()
    }
}

impl Schema for () {}
