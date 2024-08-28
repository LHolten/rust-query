use rust_query::ops::Db;

fn main() {}

fn test<'a: 'b, 'b>(val: Db<'a, ()>) -> Db<'b, ()> {
    val
}
