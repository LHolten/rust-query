#![no_main]

use libfuzzer_sys::fuzz_target;
use rust_query::private::{getToken, Token};

fuzz_target!(|bytes: &[u8]| {
    let mut f = sqlite3_parser::lexer::Scanner::new(sqlite3_parser::lexer::sql::Tokenizer::new());
    let res = f.scan(bytes);
    let res2 = getToken(bytes);
    let context = || {
        format!(
            "input: {}
        our output: {}
        their output: {}",
            String::from_utf8_lossy(bytes),
            res2.as_ref()
                .map(|(a, b, c)| format!("{a}..{c} => {b:?}"))
                .unwrap_or("none".to_owned()),
            res.as_ref()
                .map(|(a, b, c)| format!(
                    "{a}..{c} => {}",
                    b.map(|(_, a)| format!("{a:?}"))
                        .unwrap_or("none".to_owned())
                ))
                .unwrap_or_else(|e| e.to_string())
        )
    };
    match &res {
        Ok((start, v, end)) => match v {
            Some(_) => {
                let Some(res2) = &res2 else {
                    panic!("{}", context())
                };
                assert_eq!(start, &res2.0, "{}", context());
                assert_eq!(end, &res2.2, "{}", context());
                assert_ne!(res2.1, Token::TK_ILLEGAL);
            }
            None => {
                assert_eq!(res2, None)
            }
        },
        Err(_) => {
            let Some((_, token, _)) = &res2 else {
                panic!("{}", context())
            };
            if token == &Token::TK_QNUMBER {
                // sqlite_parser gives an error if `_` does not have a decimal before and after.
                // this is not in sqlite source, so we ignore this case.
                return;
            }
            assert_eq!(token, &Token::TK_ILLEGAL)
        }
    }
});
