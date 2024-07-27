use rust_query::Db;

fn main() {}

fn test<'a: 'b, 'b>(val: Db<'a, ()>) -> Db<'b, ()> {
    val
}
