#![no_main]

use libfuzzer_sys::fuzz_target;
use rust_query::private::{getToken, Token};

fuzz_target!(|bytes: &[u8]| {
    let mut f = sqlite3_parser::lexer::Scanner::new(sqlite3_parser::lexer::sql::Tokenizer::new());
    let res = f.scan(bytes);
    let res2 = getToken(bytes);
    match res {
        Ok((start, v, end)) => match v {
            Some(v) => {
                let res2 = res2.unwrap();
                assert_eq!(
                    start,
                    res2.0,
                    "{bytes:#?}, {:?}, {:?}, {:?}",
                    String::from_utf8_lossy(bytes),
                    v.1,
                    res2.1
                );
                assert_eq!(
                    end,
                    res2.2,
                    "{bytes:#?}, {}, {:?}, {:?}",
                    String::from_utf8_lossy(bytes),
                    v.1,
                    res2.1
                );
                assert_ne!(res2.1, Token::TK_ILLEGAL);
            }
            None => {
                assert_eq!(res2, None)
            }
        },
        Err(e) => {
            println!("{}", String::from_utf8_lossy(bytes));
            println!("{e}");
            let token = res2.unwrap().1;
            if token == Token::TK_QNUMBER {
                // sqlite_parser gives an error if `_` does not have a decimal before and after.
                // this is not in sqlite source, so we ignore this case.
                return;
            }
            assert_eq!(Token::TK_ILLEGAL, token)
        }
    }
});
