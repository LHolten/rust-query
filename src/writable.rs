use crate::{
    lower::ord_rc::OrdRc,
    value::{DbTyp, DbTypOpt},
};

#[derive(Default)]
pub struct Reader {
    pub(crate) builder: Vec<(&'static str, OrdRc<rusqlite::types::Value>)>,
}

impl Reader {
    pub fn col<T: DbTypOpt>(&mut self, name: &'static str, val: T) {
        let val = if let Some(val) = T::to_option(val) {
            T::Actual::out_to_value(val).into()
        } else {
            rusqlite::types::Value::Null
        };
        self.builder.push((name, OrdRc::new(val)));
    }
}
