use std::{
    fmt::{self, Display, Write, write},
    mem::replace,
};

pub struct ListWriter<'a, W: ?Sized> {
    writer: &'a mut W,
    any_items: bool,
    separator: &'static str,
}
impl<'a, W: ?Sized + Write> ListWriter<'a, W> {
    pub fn new(writer: &'a mut W, separator: &'static str) -> Self {
        Self {
            writer,
            any_items: false,
            separator,
        }
    }
    pub fn item(&mut self) -> Result<&mut W, fmt::Error> {
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
