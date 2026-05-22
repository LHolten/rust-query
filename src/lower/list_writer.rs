use std::{
    fmt::{self, Display, Write},
    mem::replace,
};

use crate::lower::emit::Stmt;

pub struct ListWriter<'a> {
    writer: &'a mut Stmt,
    any_items: bool,
    separator: &'static str,
}
impl<'a> ListWriter<'a> {
    pub fn new(writer: &'a mut Stmt, separator: &'static str) -> Self {
        Self {
            writer,
            any_items: false,
            separator,
        }
    }
    pub fn item(&mut self) -> &mut Stmt {
        if replace(&mut self.any_items, true) {
            self.writer.write(self.separator);
        }
        &mut self.writer
    }
    pub fn default(mut self, val: impl Display) {
        if !self.any_items {
            self.writer.write(val);
        }
    }
}

pub struct Alias<'a>(pub &'a str);
impl Display for Alias<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char('"')?;
        let mut input = self.0;
        while let Some(pos) = input.find('"') {
            f.write_str(&input[..pos + 1])?;
            f.write_char('"')?;
            input = &input[pos + 1..];
        }
        f.write_str(input)?;
        f.write_char('"')
    }
}
