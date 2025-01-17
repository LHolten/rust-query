use rust_query::{args::Aggregate, Column};

fn columns<'outer, 'inner, S: 'static>(
    outer: Column<'outer, S, i64>,
    inner: Column<'inner, S, i64>,
    aggr: &mut Aggregate<'outer, 'inner, S>,
) {
    aggr.filter_on(&inner, &outer);
    aggr.filter_on(outer, 10);
    aggr.filter_on(10, inner);
}

fn main() {}
