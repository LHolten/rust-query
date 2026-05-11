#[expect(non_camel_case_types)]
#[derive(PartialEq, Debug)]
pub enum Token {
    TK_BITAND,
    TK_BITNOT,
    TK_BITOR,
    TK_BLOB,
    TK_COMMA,
    TK_COMMENT,
    TK_CONCAT,
    TK_DOT,
    TK_EQ,
    TK_FLOAT,
    TK_GE,
    TK_GT,
    TK_ID,
    TK_ILLEGAL,
    TK_INTEGER,
    TK_LE,
    TK_LP,
    TK_LSHIFT,
    TK_LT,
    TK_MINUS,
    TK_NE,
    TK_PLUS,
    TK_PTR,
    TK_QNUMBER,
    TK_REM,
    TK_RP,
    TK_RSHIFT,
    TK_SEMI,
    TK_SLASH,
    TK_SPACE,
    TK_STAR,
    TK_STRING,
    TK_VARIABLE,
}

use Token::*;

const SQLITE_DIGIT_SEPARATOR: u8 = b'_';

fn isxdigit(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn isdigit(byte: u8) -> bool {
    byte.is_ascii_digit()
}

fn isspace(c: u8) -> bool {
    c.is_ascii_whitespace()
}

fn id_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || !c.is_ascii() || c == b'_' || c == b'$'
}

/*
** Return the id of the next token in string (*pz). Before returning, set
** (*pz) to point to the byte following the parsed token.
*/
pub fn get_token(pz: &[u8]) -> Option<(usize, Token, usize)> {
    let mut start = 0;
    let (mut t, end);
    loop {
        let (z, token) = get_token_internal(ZeroTerminated::new(&pz[start..]));

        if z.1 == 0 {
            return None;
        }
        if !(token == TK_SPACE || token == TK_COMMENT) {
            t = token;
            end = start + z.1;
            break;
        }
        start += z.1;
    }
    if t == TK_ID || t == TK_STRING {
        t = TK_ID;
    }
    Some((start, t, end))
}

/*
** Return the length (in bytes) of the token that begins at z[0].
** Store the token type in *tokenType before returning.
*/
fn get_token_internal(z0: ZeroTerminated) -> (ZeroTerminated, Token) {
    let Some((v, mut z)) = z0.next() else {
        return (z0, TK_ILLEGAL);
    };
    if isspace(v) {
        z.take_while(isspace);
        return (z, TK_SPACE);
    }

    let token = match v {
        b'-' => match z.next() {
            Some((b'-', new)) => {
                z = new;
                z.take_while(|v| v != b'\n');
                TK_COMMENT
            }
            Some((b'>', new)) => {
                z = new;
                z.take_if(|v| v == b'>');
                TK_PTR
            }
            _ => TK_MINUS,
        },
        b'(' => TK_LP,
        b')' => TK_RP,
        b';' => TK_SEMI,
        b'+' => TK_PLUS,
        b'*' => TK_STAR,
        b'/' => {
            if let Some((b'*', new)) = z.next()
                && let Some((mut prev, new)) = new.next()
            {
                z = new;
                z.take_until_and_including(|v| {
                    let found = prev == b'*' && v == b'/';
                    prev = v;
                    found
                });
                TK_COMMENT
            } else {
                TK_SLASH
            }
        }
        b'%' => TK_REM,
        b'=' => {
            z.take_if(|v| v == b'=');
            TK_EQ
        }
        b'<' => match z.next() {
            Some((b'=', new)) => return (new, TK_LE),
            Some((b'>', new)) => return (new, TK_NE),
            Some((b'<', new)) => return (new, TK_LSHIFT),
            _ => TK_LT,
        },
        b'>' => match z.next() {
            Some((b'=', new)) => return (new, TK_GE),
            Some((b'>', new)) => return (new, TK_RSHIFT),
            _ => TK_GT,
        },
        b'!' => match z.next() {
            Some((b'=', new)) => return (new, TK_NE),
            _ => TK_ILLEGAL,
        },
        b'|' => match z.next() {
            Some((b'|', new)) => return (new, TK_CONCAT),
            _ => TK_BITOR,
        },
        b',' => TK_COMMA,
        b'&' => TK_BITAND,
        b'~' => TK_BITNOT,
        delim @ (b'\'' | b'"' | b'`') => {
            loop {
                let true = z.take_until_and_including(|x| x == delim) else {
                    return (z, TK_ILLEGAL);
                };
                if let Some((v, new)) = z.next()
                    && v == delim
                {
                    z = new;
                } else {
                    break;
                }
            }
            if delim == b'\'' { TK_STRING } else { TK_ID }
        }
        b'.' if !z.peek(isdigit) => TK_DOT,
        first @ (b'.' | b'0'..=b'9') => {
            let mut token = TK_INTEGER;
            macro_rules! assign {
                ($typ:expr) => {{
                    token = $typ;
                    true
                }};
            }

            if first == b'0'
                && let Some((b'x' | b'X', new)) = z.next()
                && let Some((v, new)) = new.next()
                && isxdigit(v)
            {
                z = new;
                z.take_while(|v| isxdigit(v) || v == SQLITE_DIGIT_SEPARATOR && assign!(TK_QNUMBER));
            } else {
                z = z0; // reset to before the first character
                z.take_while(|v| isdigit(v) || v == SQLITE_DIGIT_SEPARATOR && assign!(TK_QNUMBER));
                if let Some((b'.', new)) = z.next() {
                    z = new;
                    if token == TK_INTEGER {
                        token = TK_FLOAT
                    };
                    z.take_while(|v| {
                        isdigit(v) || v == SQLITE_DIGIT_SEPARATOR && assign!(TK_QNUMBER)
                    });
                }
                if let Some((b'e' | b'E', new)) = z.next()
                    && let Some((v, new)) = new.next()
                    && (isdigit(v) || ((v == b'+' || v == b'-') && new.peek(isdigit)))
                {
                    z = new;
                    if token == TK_INTEGER {
                        token = TK_FLOAT
                    };
                    z.take_while(|v| {
                        isdigit(v) || v == SQLITE_DIGIT_SEPARATOR && assign!(TK_QNUMBER)
                    });
                }
            }
            z.take_while(|v| id_char(v) && assign!(TK_ILLEGAL));
            token
        }
        b'[' => {
            let found = z.take_until_and_including(|v| v == b']');
            if found { TK_ID } else { TK_ILLEGAL }
        }
        b'?' => {
            z.take_while(isdigit);
            TK_VARIABLE
        }
        b'$' | b'@' | b'#' | b':' => {
            let n = z.take_while(id_char);
            if n == 0 { TK_ILLEGAL } else { TK_VARIABLE }
        }
        b'x' | b'X' if let Some((b'\'', new)) = z.next() => {
            z = new;
            let count = z.take_while(isxdigit);
            if let Some((b'\'', new)) = z.next()
                && count % 2 == 0
            {
                z = new;
                TK_BLOB
            } else {
                z.take_until_and_including(|v| v == b'\'');
                TK_ILLEGAL
            }
        }
        0xef if let Some((0xbb, new)) = z.next()
            && let Some((0xbf, new)) = new.next() =>
        {
            z = new;
            TK_SPACE
        }
        v if id_char(v) => {
            z.take_while(id_char);
            TK_ID
        }
        _ => TK_ILLEGAL,
    };
    (z, token)
}

struct ZeroTerminated<'x>(&'x [u8], pub usize);

impl<'x> ZeroTerminated<'x> {
    fn new(slice: &'x [u8]) -> Self {
        Self(slice, 0)
    }

    fn next(&self) -> Option<(u8, Self)> {
        let v = *self.0.get(self.1)?;
        Some((v, Self(self.0, self.1 + 1)))
    }

    fn take_while(&mut self, mut f: impl FnMut(u8) -> bool) -> usize {
        let mut count = 0;
        while let Some((v, new)) = self.next()
            && f(v)
        {
            *self = new;
            count += 1;
        }
        count
    }

    fn take_until_and_including(&mut self, mut f: impl FnMut(u8) -> bool) -> bool {
        loop {
            let Some((v, new)) = self.next() else {
                return false;
            };
            *self = new;
            if f(v) {
                return true;
            }
        }
    }

    fn take_if(&mut self, f: impl FnOnce(u8) -> bool) {
        if let Some((v, new)) = self.next()
            && f(v)
        {
            *self = new;
        }
    }

    fn peek(&self, f: impl FnOnce(u8) -> bool) -> bool {
        if let Some((v, _)) = self.next()
            && f(v)
        {
            true
        } else {
            false
        }
    }
}
