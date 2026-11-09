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

impl rust_query::private::RefinementTyp for Quality {
    type RBase = String;

    fn r_out_to_value(self) -> Self::RBase {
        match self {
            Quality::Foo => "foo",
            Quality::Bar => "bar",
        }
        .to_owned()
    }

    fn r_from_sql(value: <Self::RBase as rust_query::private::BaseTyp>::Ref<'_>) -> Self {
        match value {
            "foo" => Quality::Foo,
            "bar" => Quality::Bar,
            _ => unreachable!(),
        }
    }

    fn r_check(col: rust_query::private::Alias) -> String {
        format!("{col} IN ('foo', 'bar')")
    }
}

impl<'column, S> rust_query::IntoExpr<'column, S> for Quality {
    type Typ = Quality;

    fn into_expr(&self) -> rust_query::Expr<'column, S, Self::Typ> {
        rust_query::private::refinement_to_expr(self.clone())
    }
}
