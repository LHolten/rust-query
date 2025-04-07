use sea_query::{Alias, ExprTrait, extension::sqlite::SqliteExpr};

use super::{EqTyp, Expr, IntoExpr, NumTyp, Typed};

impl<'column, S, T: NumTyp> Expr<'column, S, T> {
    /// Add two expressions together.
    pub fn add(&self, rhs: impl IntoExpr<'column, S, Typ = T>) -> Expr<'column, S, T> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(move |b| lhs.build_expr(b).add(rhs.build_expr(b)))
    }

    /// Compute the less than operator of two expressions.
    pub fn lt(&self, rhs: impl IntoExpr<'column, S, Typ = T>) -> Expr<'column, S, bool> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(move |b| lhs.build_expr(b).lt(rhs.build_expr(b)))
    }
}

impl<'column, S, T: EqTyp + 'static> Expr<'column, S, T> {
    /// Check whether two expressions are equal.
    pub fn eq(&self, rhs: impl IntoExpr<'column, S, Typ = T>) -> Expr<'column, S, bool> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(move |b| lhs.build_expr(b).eq(rhs.build_expr(b)))
    }
}

impl<'column, S> Expr<'column, S, bool> {
    /// Checks whether an expression is false.
    pub fn not(&self) -> Expr<'column, S, bool> {
        let val = self.inner.clone();
        Expr::adhoc(move |b| val.build_expr(b).not())
    }

    /// Check if two expressions are both true.
    pub fn and(&self, rhs: impl IntoExpr<'column, S, Typ = bool>) -> Expr<'column, S, bool> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(move |b| lhs.build_expr(b).and(rhs.build_expr(b)))
    }

    /// Check if one of two expressions is true.
    pub fn or(&self, rhs: impl IntoExpr<'column, S, Typ = bool>) -> Expr<'column, S, bool> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(move |b| lhs.build_expr(b).or(rhs.build_expr(b)))
    }
}

impl<'column, S, Typ: 'static> Expr<'column, S, Option<Typ>> {
    /// Use the first expression if it is [Some], otherwise use the second expression.
    pub fn unwrap_or(&self, rhs: impl IntoExpr<'column, S, Typ = Typ>) -> Expr<'column, S, Typ>
    where
        Self: IntoExpr<'column, S, Typ = Option<Typ>>,
    {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(move |b| sea_query::Expr::expr(lhs.build_expr(b)).if_null(rhs.build_expr(b)))
    }

    /// Check that the expression is [Some].
    pub fn is_some(&self) -> Expr<'column, S, bool> {
        let val = self.inner.clone();
        Expr::adhoc(move |b| val.build_expr(b).is_not_null())
    }
}

impl<'column, S> Expr<'column, S, i64> {
    /// Convert the [i64] expression to [f64] type.
    pub fn as_float(&self) -> Expr<'column, S, f64> {
        let val = self.inner.clone();
        Expr::adhoc(move |b| val.build_expr(b).cast_as(Alias::new("real")))
    }
}

// This is a copy of the function from the glob crate https://github.com/rust-lang/glob/blob/49ee1e92bd6e8c5854c0b339634f9b4b733aba4f/src/lib.rs#L720-L737.
fn escape_glob(s: impl AsRef<str>) -> String {
    let mut escaped = String::new();
    for c in s.as_ref().chars() {
        match c {
            // note that ! does not need escaping because it is only special
            // inside brackets
            '?' | '*' | '[' | ']' => {
                escaped.push('[');
                escaped.push(c);
                escaped.push(']');
            }
            c => {
                escaped.push(c);
            }
        }
    }
    escaped
}

impl<'column, S> Expr<'column, S, String> {
    /// Check if the expression starts with the string pattern.
    ///
    /// Matches case-sensitive. The pattern gets automatically escaped.
    pub fn starts_with(&self, pattern: impl AsRef<str>) -> Expr<'column, S, bool> {
        self.glob(format!("{}*", escape_glob(pattern)))
    }

    /// Check if the expression ends with the string pattern.
    ///
    /// Matches case-sensitive. The pattern gets automatically escaped.
    pub fn ends_with(&self, pattern: impl AsRef<str>) -> Expr<'column, S, bool> {
        self.glob(format!("*{}", escape_glob(pattern)))
    }

    /// Check if the expression contains the string pattern.
    ///
    /// Matches case-sensitive. The pattern gets automatically escaped.
    pub fn contains(&self, pattern: impl AsRef<str>) -> Expr<'column, S, bool> {
        self.glob(format!("*{}*", escape_glob(pattern)))
    }

    /// Check if the expression matches the pattern [docs](https://www.sqlite.org/lang_expr.html#like).
    ///
    /// This is a case-sensitive version of [like](Self::like). It uses Unix file globbing syntax for wild
    /// cards. `*` matches any sequence of characters and `?` matches any single character. `[0-9]` matches
    /// any single digit and `[a-z]` matches any single lowercase letter. `^` negates the pattern.
    pub fn glob(&self, rhs: impl IntoExpr<'column, S, Typ = String>) -> Expr<'column, S, bool> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(move |b| sea_query::Expr::expr(lhs.build_expr(b)).glob(rhs.build_expr(b)))
    }

    /// Check if the expression matches the pattern [docs](https://www.sqlite.org/lang_expr.html#like).
    ///
    /// As noted in the docs, it is **case-insensitive** for ASCII characters. Other characters are case-sensitive.
    /// For creating patterns it uses `%` as a wildcard for any sequence of characters and `_` for any single character.
    /// Special characters should be escaped with `\`.
    pub fn like(&self, pattern: impl Into<String>) -> Expr<'column, S, bool> {
        let lhs = self.inner.clone();
        let rhs = pattern.into();
        Expr::adhoc(move |b| {
            sea_query::Expr::expr(lhs.build_expr(b))
                .like(sea_query::LikeExpr::new(&rhs).escape('\\'))
        })
    }
}
