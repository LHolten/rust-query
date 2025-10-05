use crate::{Table, value::DynTypedExpr};

pub trait Joinable<'inner> {
    type Typ: Table;
    fn conds(self) -> Vec<(&'static str, DynTypedExpr)>;
}
