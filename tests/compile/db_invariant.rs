use rust_query::ops::Join;

fn main() {}

fn test<'a: 'b, 'b>(val: Join<'a, ()>) -> Join<'b, ()> {
    val
}
