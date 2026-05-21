use crate::{lower::ord_rc::OrdRc, value::DbTyp};

#[derive(Default)]
pub struct Reader {
    pub(crate) builder: Vec<(&'static str, OrdRc<rusqlite::types::Value>)>,
}

impl Reader {
    pub fn col<T: DbTyp>(&mut self, name: &'static str, val: T) {
        self.builder.push((name, T::out_to_value(val).into()));
    }
}
