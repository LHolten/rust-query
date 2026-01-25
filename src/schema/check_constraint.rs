use std::fmt::Display;

use crate::private::{Token, get_token};

use crate::lower;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum TokenTree {
    Token(String),
    // vec is separated by commas,
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

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Parsed {
    // vec is separated by spaces
    tokens: Vec<TokenTree>,
}

impl Display for Parsed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let formatted: Vec<_> = self.tokens.iter().map(|x| x.to_string()).collect();
        write!(f, "{}", formatted.join(" "))
    }
}

impl Parsed {
    pub fn parse(sql: &str) -> Self {
        Self {
            tokens: parse_sql_tree(sql),
        }
    }
}

fn parse_sql_subtree(sql: &mut &str) -> Vec<TokenTree> {
    let mut items = Vec::new();
    loop {
        match get_token(sql.as_bytes()) {
            None | Some((_, Token::TK_COMMA | Token::TK_RP, _)) => break,
            Some((start, token_type, end)) => {
                let text = &sql[start..end];
                *sql = &sql[end..];

                if token_type == Token::TK_LP {
                    let mut inner_items = vec![];
                    loop {
                        let tokens = parse_sql_subtree(sql);
                        inner_items.push(Parsed { tokens });

                        let (_, next_token, end) = get_token(sql.as_bytes()).unwrap();
                        *sql = &sql[end..];

                        if next_token == Token::TK_RP {
                            break;
                        } else {
                            assert_eq!(next_token, Token::TK_COMMA);
                        }
                    }
                    items.push(TokenTree::Group(inner_items));
                } else {
                    items.push(TokenTree::Token(text.to_owned()));
                };
            }
        }
    }
    items
}

fn parse_sql_tree(mut sql: &str) -> Vec<TokenTree> {
    let res = parse_sql_subtree(&mut sql);
    assert!(sql.is_empty());
    res
}

pub fn get_check_constraint(sql: &str, col: &str) -> Option<Parsed> {
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

    let col_encoded = lower::list_writer::Alias(col).to_string();
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
    Some(check)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_some_sql() {
        let sql = r#"CREATE TABLE IF NOT EXISTS "customer" ( "address" text NOT NULL, "city" text NOT NULL, "company" text NULL, "country" text NOT NULL, "email" text NOT NULL, "fax" text NULL, "first_name" text NOT NULL, "last_name" text NOT NULL, "phone" integer NULL, "postal_code" text NULL, "some_bool" integer NOT NULL CHECK ("some_bool" IN (0, 1)), "state" text NULL, "support_rep" integer NOT NULL, "id" integer PRIMARY KEY, FOREIGN KEY ("support_rep") REFERENCES "employee" ("id") ) STRICT;"#;
        assert_eq!(
            get_check_constraint(sql, "some_bool"),
            Some(Parsed::parse(r#""some_bool" IN (0, 1)"#))
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
