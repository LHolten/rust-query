use std::{
    fmt::{self, Write},
    mem::replace,
};

pub struct ListWriter<'a> {
    writer: &'a mut dyn Write,
    any_items: bool,
    separator: &'static str,
}
impl<'a> ListWriter<'a> {
    pub fn new(writer: &'a mut dyn Write, separator: &'static str) -> Self {
        Self {
            writer,
            any_items: false,
            separator,
        }
    }
    pub fn item(&mut self) -> Result<&mut dyn Write, fmt::Error> {
        if replace(&mut self.any_items, true) {
            write!(&mut self.writer, "{}", self.separator)?;
        }
        Ok(&mut self.writer)
    }
    pub fn default(mut self, val: &str) -> fmt::Result {
        if !self.any_items {
            write!(&mut self.writer, "{val}")?;
        }
        Ok(())
    }
}
