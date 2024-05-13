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

#[cfg(test)]
mod tests {
    use crate::Row;

    use super::*;

    struct M<X: for<'x, 'a> FnMut(Row<'x, 'a>)> {
        x: X,
    }

    struct M2 {
        x: for<'x, 'a> fn(Row<'x, 'a>),
    }

    impl<X: for<'x, 'a> FnMut(Row<'x, 'a>)> Migration<()> for M<X> {
        type S = ();
    }

    impl Migration<()> for M2 {
        type S = ();
    }

    #[test]
    fn test_name() {
        ().migrate(|schema| M { x: |_x| () });
    }
}
