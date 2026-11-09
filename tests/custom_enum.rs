use rust_query::migration::schema;

fn main() {}

#[schema(Schema)]
pub mod vN {
    use crate::Quality;

    pub struct Item {
        pub name: String,
        pub quality: Quality,
    }
}

#[derive(Clone)]
pub enum Quality {
    Foo,
    Bar,
}

rust_query::private::refinement_typ!(
    Quality,
    String,
    |val| match val {
        Quality::Foo => "foo",
        Quality::Bar => "bar",
    }
    .to_owned(),
    |value| match value {
        "foo" => Quality::Foo,
        "bar" => Quality::Bar,
        _ => unreachable!(),
    },
    |col| format!("{col} IN ('foo', 'bar')"),
);
