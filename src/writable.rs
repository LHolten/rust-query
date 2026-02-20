use crate::value::DbTyp;

#[derive(Default)]
pub struct Reader {
    pub(crate) builder: Vec<(&'static str, sea_query::Expr)>,
}


impl Reader {
    pub fn col<T: DbTyp>(&mut self, name: &'static str, val: T) {
        self.builder.push((name, T::out_to_value(val).into()));
    }
}
