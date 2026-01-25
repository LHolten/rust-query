use crate::value::MyTyp;

pub struct Reader {
    pub(crate) builder: Vec<(&'static str, sea_query::Expr)>,
}

impl Default for Reader {
    fn default() -> Self {
        Self {
            builder: Default::default(),
        }
    }
}

impl Reader {
    pub fn col<T: MyTyp>(&mut self, name: &'static str, val: T::Out) {
        self.builder.push((name, T::out_to_value(val).into()));
    }
}
