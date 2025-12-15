enum TokenTree {
    Token(String),
    // outer vec is separated by commas,
    // inner vec is separated by spaces
    Group(Vec<Vec<TokenTree>>),
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
fn parse_sql_tree(mut sql: &str) -> Vec<TokenTree> {
    let mut items = Vec::new();

    loop {
        match &sql.get(..1) {
            Some(" ") => sql = &sql[1..],
            Some("(") => {
                sql = &sql[1..];
                let mut inner_items = vec![];
                loop {
                    let len = find_skip_quotes(&sql, |c| c == ',' || c == ')');
                    inner_items.push(parse_sql_tree(&sql[..len]));
                    sql = &sql[len..];

                    if &sql[..1] == ")" {
                        break;
                    }
                }
                sql = &sql[1..];
                items.push(TokenTree::Group(inner_items));
            }
            Some(_) => {
                let until = find_skip_quotes(sql, |c| c == ' ');
                items.push(TokenTree::Token(sql[..until].to_owned()));
                sql = &sql[until..];
            }
            None => break,
        }
    }
    items
}

#[allow(unused)]
pub fn get_check_constraint(sql: &str, col: &str) -> String {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_some_sql() {
        let sql = r#"CREATE TABLE IF NOT EXISTS "customer" ( "address" text NOT NULL, "city" text NOT NULL, "company" text NULL, "country" text NOT NULL, "email" text NOT NULL, "fax" text NULL, "first_name" text NOT NULL, "last_name" text NOT NULL, "phone" integer NULL, "postal_code" text NULL, "some_bool" integer NOT NULL CHECK ("some_bool" IN (0, 1)), "state" text NULL, "support_rep" integer NOT NULL, "id" integer PRIMARY KEY, FOREIGN KEY ("support_rep") REFERENCES "employee" ("id") ) STRICT;"#;
        assert_eq!(
            get_check_constraint(sql, "some_bool"),
            r#""some_bool" IN (0, 1)"#
        )
    }
}
