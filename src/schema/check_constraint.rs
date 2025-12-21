use std::fmt::Display;

use sea_query::{IntoColumnRef, QueryBuilder};
use sqlite3_parser::lexer::{Scanner, sql::Tokenizer};

#[derive(PartialEq, Debug)]
enum TokenTree {
    Token(String),
    // outer vec is separated by commas,
    // inner vec is separated by spaces
    Group(Vec<Parsed>),
}
impl Display for TokenTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenTree::Token(x) => f.write_str(x),
            TokenTree::Group(parsed) => {
                let formatted: Vec<_> = parsed.iter().map(|x| x.to_string()).collect();
                write!(f, "({})", formatted.join(", "))
            }
        }
    }
}

#[derive(PartialEq, Debug)]
struct Parsed {
    tokens: Vec<TokenTree>,
}

impl Display for Parsed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let formatted: Vec<_> = self.tokens.iter().map(|x| x.to_string()).collect();
        write!(f, "{}", formatted.join(" "))
    }
}

fn parse_sql_subtree(tokenizer: &mut Scanner<Tokenizer>, sql: &str) -> Vec<TokenTree> {
    use sqlite3_parser::dialect::TokenType;
    let mut items = Vec::new();
    loop {
        tokenizer.mark();
        match tokenizer.scan(sql.as_bytes()).unwrap() {
            (_, None, _) => break,
            (start, Some((_, token_type)), end) => match token_type {
                TokenType::TK_LP => {
                    let mut inner_items = vec![];
                    loop {
                        let tokens = parse_sql_subtree(tokenizer, sql);
                        let next_token = tokenizer.scan(sql.as_bytes()).unwrap().1.unwrap().1;
                        inner_items.push(Parsed { tokens });

                        if next_token == TokenType::TK_RP {
                            break;
                        }
                        assert!(!sql.is_empty())
                    }
                    items.push(TokenTree::Group(inner_items));
                }
                TokenType::TK_COMMA | TokenType::TK_RP => {
                    tokenizer.reset_to_mark();
                    break;
                }
                _ => {
                    items.push(TokenTree::Token(sql[start..end].to_owned()));
                }
            },
        }
    }
    items
}

fn parse_sql_tree(sql: &str) -> Vec<TokenTree> {
    let mut f = sqlite3_parser::lexer::Scanner::new(sqlite3_parser::lexer::sql::Tokenizer::new());
    let res = parse_sql_subtree(&mut f, sql);
    assert!(f.scan(sql.as_bytes()).unwrap().1.is_none());
    res
}

pub fn get_check_constraint(sql: &str, col: &str) -> Option<String> {
    let tokens = parse_sql_tree(sql);
    let mut columns = tokens
        .into_iter()
        .find_map(|x| {
            if let TokenTree::Group(g) = x {
                Some(g)
            } else {
                None
            }
        })
        .expect("expected column defs");

    let mut col_encoded = String::new();
    sea_query::SqliteQueryBuilder.prepare_column_ref(
        &sea_query::Alias::new(col).into_column_ref(),
        &mut col_encoded,
    );
    let col_token = TokenTree::Token(col_encoded);
    let col_token_alt = TokenTree::Token(col.to_owned());

    let pos = columns
        .iter()
        .position(|x| x.tokens[0] == col_token)
        // TODO: maybe make this more strict?
        .or_else(|| columns.iter().position(|x| x.tokens[0] == col_token_alt))
        .expect("column should exist");
    let mut col_def = columns.swap_remove(pos).tokens;

    let idx = col_def
        .iter()
        .position(|x| *x == TokenTree::Token("CHECK".to_owned()))?;
    let TokenTree::Group(check) = col_def.remove(idx + 1) else {
        panic!("expected group after CHECK")
    };
    let [check] = check.try_into().unwrap();
    Some(check.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_some_sql() {
        let sql = r#"CREATE TABLE IF NOT EXISTS "customer" ( "address" text NOT NULL, "city" text NOT NULL, "company" text NULL, "country" text NOT NULL, "email" text NOT NULL, "fax" text NULL, "first_name" text NOT NULL, "last_name" text NOT NULL, "phone" integer NULL, "postal_code" text NULL, "some_bool" integer NOT NULL CHECK ("some_bool" IN (0, 1)), "state" text NULL, "support_rep" integer NOT NULL, "id" integer PRIMARY KEY, FOREIGN KEY ("support_rep") REFERENCES "employee" ("id") ) STRICT;"#;
        assert_eq!(
            get_check_constraint(sql, "some_bool").as_deref(),
            Some(r#""some_bool" IN (0, 1)"#)
        )
    }

    #[test]
    fn parse_some_more() {
        let item = r#"CREATE TABLE execution (
        id INTEGER PRIMARY KEY,
        timestamp INTEGER NOT NULL DEFAULT (unixepoch('now')),
        fuel_used INTEGER NOT NULL,
        -- answer can be null if the solution crashed
        answer INTEGER,
        instance INTEGER NOT NULL REFERENCES instance,
        solution INTEGER NOT NULL REFERENCES solution,
        UNIQUE (instance, solution)
    ) STRICT"#;
        let parsed = parse_sql_tree(item);
        expect_test::expect_file!["parse_result.dbg"].assert_debug_eq(&parsed);
    }
}
