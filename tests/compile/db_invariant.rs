use rust_query::DynValue;

fn main() {}

fn test<'a: 'b, 'b>(val: DynValue<'a, (), ()>) -> DynValue<'b, (), ()> {
    val
}
