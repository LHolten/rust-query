use crate::{Expr, rows::Rows, value::MyTyp};

pub trait Joinable<'inner, S> {
    type Typ: MyTyp;
    fn apply(self, rows: &mut Rows<'inner, S>) -> Expr<'inner, S, Self::Typ>;
}
