use rust_query::Column;

fn main() {}

fn test<'a: 'b, 'b>(val: Column<'a, (), ()>) -> Column<'b, (), ()> {
    val
}
