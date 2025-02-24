use rust_query::{args::Aggregate, Expr};

fn columns<'outer, 'inner, S: 'static>(
    outer: Expr<'outer, S, i64>,
    inner: Expr<'inner, S, i64>,
    aggr: &mut Aggregate<'outer, 'inner, S>,
) {
    aggr.filter_on(&inner, &outer);
    aggr.filter_on(outer, 10);
    aggr.filter_on(10, inner);
}

fn sum<'outer, 'inner, S: 'static>(
    outer: Expr<'outer, S, i64>,
    aggr: &Aggregate<'outer, 'inner, S>,
) {
    aggr.sum(outer);
}

fn main() {}
