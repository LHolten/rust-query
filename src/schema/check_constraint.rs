use sea_query::{IntoColumnRef, QueryBuilder};

#[derive(PartialEq, Debug)]
enum TokenTree {
    Token(String),
    // outer vec is separated by commas,
    // inner vec is separated by spaces
    Group(Vec<Parsed>),
}
#[derive(PartialEq, Debug)]
struct Parsed {
    original: String,
    tokens: Vec<TokenTree>,
}

/// get first location of char, will return string len if not found.
/// skips over aliases and string using the fact that they are quoted and
/// quotes inside aliases and strings are duplicated.
fn find_skip_quotes(sql: &str, pat: impl Fn(char) -> bool) -> usize {
    let mut pos = 0;
    let mut total_double_quotes = 0;
    let mut total_single_quotes = 0;
    loop {
        let new = sql[pos..].find(&pat).map(|x| x + pos).unwrap_or(sql.len());
        total_double_quotes += sql[pos..new].chars().filter(|x| *x == '"').count();
        total_single_quotes += sql[pos..new].chars().filter(|x| *x == '\'').count();
        pos = new;
        if total_double_quotes % 2 == 0 && total_single_quotes % 2 == 0 {
            return pos;
        }
        assert!(pos != sql.len())
    }
}

/// This parses a piece of sql into a token tree assuming that it is well formated.
fn parse_sql_subtree(sql: &mut &str) -> Vec<TokenTree> {
    let mut items = Vec::new();
    loop {
        match &sql.get(..1) {
            None | Some("," | ")") => break,
            Some(" ") => *sql = &sql[1..],
            Some("(") => {
                *sql = &sql[1..];
                let mut inner_items = vec![];
                loop {
                    let old_sql = *sql;
                    let tokens = parse_sql_subtree(sql);
                    inner_items.push(Parsed {
                        original: old_sql[..old_sql.len() - sql.len()].to_owned(),
                        tokens,
                    });
                    let end_char = &sql[..1];
                    *sql = &sql[1..];

                    if end_char == ")" {
                        break;
                    }
                    assert!(!sql.is_empty())
                }
                items.push(TokenTree::Group(inner_items));
            }
            Some(_) => {
                let until = find_skip_quotes(sql, |c| c == ' ' || c == ',' || c == ')');
                items.push(TokenTree::Token(sql[..until].to_owned()));
                *sql = &sql[until..];
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

#[expect(unused)]
pub fn get_check_constraint(sql: &str, col: &str) -> Option<String> {
    let tokens = parse_sql_tree(sql);
    let columns = tokens
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
    let mut col_token = TokenTree::Token(col_encoded);

    let mut col_def = columns
        .into_iter()
        .find(|x| x.tokens[0] == col_token)
        .expect("column should exist")
        .tokens;

    let idx = col_def
        .iter()
        .position(|x| *x == TokenTree::Token("CHECK".to_owned()))?;
    let TokenTree::Group(check) = col_def.remove(idx + 1) else {
        panic!("expected group after CHECK")
    };
    let [check] = check.try_into().unwrap();
    Some(check.original)
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
}
