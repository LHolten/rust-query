use sea_query::{Alias, Asterisk, Expr, NullAlias, Query, SelectStatement, SimpleExpr};

use crate::value::MyAlias;

// pub(super) enum Operation {
//     From(Source),
//     Filter(SimpleExpr),
//     // distinct on first expr and sort on second expr
//     // Distinct(SimpleExpr, SimpleExpr),
// }

#[derive(Default)]
pub struct MySelect {
    // the sources to use
    pub(super) sources: Vec<Source>,
    // all conditions to check
    pub(super) filters: Vec<SimpleExpr>,
    // distinct on
    pub(super) group: Vec<(MyAlias, SimpleExpr)>,
    // calculating these agregates
    pub(super) aggr: Vec<(MyAlias, SimpleExpr)>,
    // sort on value (and keep row with smallest value)
    pub(super) sort: Vec<(MyAlias, SimpleExpr, bool)>,
}

pub struct MyDef {
    pub(super) table: &'static str,
    pub(super) columns: Vec<(&'static str, MyAlias)>,
}

pub(super) enum Source {
    Select(MySelect),
    Table(MyDef),
}
