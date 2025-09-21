use sea_query::{Alias, ExprTrait, extension::sqlite::SqliteExpr};

use crate::value::MyTyp;

use super::{EqTyp, Expr, IntoExpr, NumTyp};

impl<'column, S, T: NumTyp> Expr<'column, S, T> {
    /// Add two expressions together.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(1.into_expr().add(2)), 3);
    /// assert_eq!(txn.query_one(1.0.into_expr().add(2.0)), 3.0);
    /// # });
    /// ```
    pub fn add(&self, rhs: impl IntoExpr<'column, S, Typ = T>) -> Expr<'column, S, T> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(move |b| lhs.build_expr(b).add(rhs.build_expr(b)))
    }

    /// Subtract one expression from another.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(1.into_expr().sub(2)), -1);
    /// assert_eq!(txn.query_one(1.0.into_expr().sub(2.0)), -1.0);
    /// # });
    /// ```
    pub fn sub(&self, rhs: impl IntoExpr<'column, S, Typ = T>) -> Expr<'column, S, T> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(move |b| lhs.build_expr(b).sub(rhs.build_expr(b)))
    }

    /// Multiply two expressions together.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(2.into_expr().mul(3)), 6);
    /// assert_eq!(txn.query_one(2.0.into_expr().mul(3.0)), 6.0);
    /// # });
    /// ```
    pub fn mul(&self, rhs: impl IntoExpr<'column, S, Typ = T>) -> Expr<'column, S, T> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(move |b| lhs.build_expr(b).mul(rhs.build_expr(b)))
    }

    /// Compute the less than operator (<) of two expressions.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(2.into_expr().lt(3)), true);
    /// assert_eq!(txn.query_one(1.into_expr().lt(1)), false);
    /// assert_eq!(txn.query_one(3.0.into_expr().lt(1.0)), false);
    /// # });
    /// ```
    pub fn lt(&self, rhs: impl IntoExpr<'column, S, Typ = T>) -> Expr<'column, S, bool> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(move |b| lhs.build_expr(b).lt(rhs.build_expr(b)))
    }

    /// Compute the less than or equal operator (<=) of two expressions.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(2.into_expr().lte(2)), true);
    /// assert_eq!(txn.query_one(3.0.into_expr().lte(1.0)), false);
    /// # });
    /// ```
    pub fn lte(&self, rhs: impl IntoExpr<'column, S, Typ = T>) -> Expr<'column, S, bool> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(move |b| lhs.build_expr(b).lte(rhs.build_expr(b)))
    }

    /// Compute the greater than operator (>) of two expressions.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(2.into_expr().gt(2)), false);
    /// assert_eq!(txn.query_one(3.0.into_expr().gt(1.0)), true);
    /// # });
    /// ```
    pub fn gt(&self, rhs: impl IntoExpr<'column, S, Typ = T>) -> Expr<'column, S, bool> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(move |b| lhs.build_expr(b).gt(rhs.build_expr(b)))
    }

    /// Compute the greater than or equal (>=) operator of two expressions.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(2.into_expr().gte(3)), false);
    /// assert_eq!(txn.query_one(3.0.into_expr().gte(3.0)), true);
    /// # });
    /// ```
    pub fn gte(&self, rhs: impl IntoExpr<'column, S, Typ = T>) -> Expr<'column, S, bool> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(move |b| lhs.build_expr(b).gte(rhs.build_expr(b)))
    }
}

impl<'column, S, T: EqTyp + 'static> Expr<'column, S, T> {
    /// Check whether two expressions are equal.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(2.into_expr().eq(2)), true);
    /// assert_eq!(txn.query_one(3.0.into_expr().eq(3.0)), true);
    /// assert_eq!(txn.query_one("test".into_expr().eq("test")), true);
    /// assert_eq!(txn.query_one(b"test".into_expr().eq(b"test" as &[u8])), true);
    /// assert_eq!(txn.query_one(false.into_expr().eq(false)), true);
    ///
    /// assert_eq!(txn.query_one(1.into_expr().eq(2)), false);
    /// # });
    /// ```
    pub fn eq(&self, rhs: impl IntoExpr<'column, S, Typ = T>) -> Expr<'column, S, bool> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(move |b| lhs.build_expr(b).eq(rhs.build_expr(b)))
    }
}

impl<'column, S> Expr<'column, S, bool> {
    /// Checks whether an expression is false.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(true.into_expr().not()), false);
    /// assert_eq!(txn.query_one(false.into_expr().not()), true);
    /// # });
    /// ```
    pub fn not(&self) -> Expr<'column, S, bool> {
        let val = self.inner.clone();
        Expr::adhoc(move |b| val.build_expr(b).not())
    }

    /// Check if two expressions are both true.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(true.into_expr().and(true)), true);
    /// assert_eq!(txn.query_one(false.into_expr().and(true)), false);
    /// assert_eq!(txn.query_one(false.into_expr().and(false)), false);
    /// # });
    /// ```
    pub fn and(&self, rhs: impl IntoExpr<'column, S, Typ = bool>) -> Expr<'column, S, bool> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(move |b| lhs.build_expr(b).and(rhs.build_expr(b)))
    }

    /// Check if one of two expressions is true.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(true.into_expr().or(true)), true);
    /// assert_eq!(txn.query_one(false.into_expr().or(true)), true);
    /// assert_eq!(txn.query_one(false.into_expr().or(false)), false);
    /// # });
    /// ```
    pub fn or(&self, rhs: impl IntoExpr<'column, S, Typ = bool>) -> Expr<'column, S, bool> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(move |b| lhs.build_expr(b).or(rhs.build_expr(b)))
    }
}

impl<'column, S, Typ: MyTyp> Expr<'column, S, Option<Typ>> {
    /// Use the first expression if it is [Some], otherwise use the second expression.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(Some(10).into_expr().unwrap_or(5)), 10);
    /// assert_eq!(txn.query_one(None::<String>.into_expr().unwrap_or("foo")), "foo");
    /// # });
    /// ```
    pub fn unwrap_or(&self, rhs: impl IntoExpr<'column, S, Typ = Typ>) -> Expr<'column, S, Typ>
    where
        Self: IntoExpr<'column, S, Typ = Option<Typ>>,
    {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(move |b| sea_query::Expr::expr(lhs.build_expr(b)).if_null(rhs.build_expr(b)))
    }

    /// Check that the expression is [Some].
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(Some(10).into_expr().is_some()), true);
    /// assert_eq!(txn.query_one(None::<i64>.into_expr().is_some()), false);
    /// # });
    /// ```
    pub fn is_some(&self) -> Expr<'column, S, bool> {
        let val = self.inner.clone();
        Expr::adhoc(move |b| val.build_expr(b).is_not_null())
    }

    /// Check that the expression is [None].
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(Some(10).into_expr().is_none()), false);
    /// assert_eq!(txn.query_one(None::<i64>.into_expr().is_none()), true);
    /// # });
    /// ```
    pub fn is_none(&self) -> Expr<'column, S, bool> {
        let val = self.inner.clone();
        Expr::adhoc(move |b| val.build_expr(b).is_null())
    }
}

impl<'column, S> Expr<'column, S, i64> {
    /// Convert the [i64] expression to [f64] type.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(10.into_expr().as_float()), 10.0);
    /// # });
    /// ```
    pub fn as_float(&self) -> Expr<'column, S, f64> {
        let val = self.inner.clone();
        Expr::adhoc(move |b| val.build_expr(b).cast_as(Alias::new("real")))
    }
}

impl<'column, S> Expr<'column, S, String> {
    /// Check if the expression starts with the string pattern.
    ///
    /// Matches case-sensitive. The pattern gets automatically escaped.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one("hello world".into_expr().starts_with("hello")), true);
    /// assert_eq!(txn.query_one("hello world".into_expr().starts_with("Hello")), false);
    /// # });
    /// ```
    pub fn starts_with(&self, pattern: impl AsRef<str>) -> Expr<'column, S, bool> {
        self.glob(format!("{}*", escape_glob(pattern)))
    }

    /// Check if the expression ends with the string pattern.
    ///
    /// Matches case-sensitive. The pattern gets automatically escaped.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one("hello world".into_expr().ends_with("world")), true);
    /// assert_eq!(txn.query_one("hello world".into_expr().ends_with("World")), false);
    /// # });
    /// ```
    pub fn ends_with(&self, pattern: impl AsRef<str>) -> Expr<'column, S, bool> {
        self.glob(format!("*{}", escape_glob(pattern)))
    }

    /// Check if the expression contains the string pattern.
    ///
    /// Matches case-sensitive. The pattern gets automatically escaped.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one("rhubarb".into_expr().contains("bar")), true);
    /// assert_eq!(txn.query_one("rhubarb".into_expr().contains("Bar")), false);
    /// # });
    /// ```
    pub fn contains(&self, pattern: impl AsRef<str>) -> Expr<'column, S, bool> {
        self.glob(format!("*{}*", escape_glob(pattern)))
    }

    /// Check if the expression matches the pattern [sqlite docs](https://www.sqlite.org/lang_expr.html#like).
    ///
    /// This is a case-sensitive version of [like](Self::like). It uses Unix file globbing syntax for wild
    /// cards. `*` matches any sequence of characters and `?` matches any single character. `[0-9]` matches
    /// any single digit and `[a-z]` matches any single lowercase letter. `^` negates the pattern.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one("hello world".into_expr().glob("?ello*")), true);
    /// assert_eq!(txn.query_one("hello world".into_expr().glob("Hell*")), false);
    /// # });
    /// ```
    pub fn glob(&self, rhs: impl IntoExpr<'column, S, Typ = String>) -> Expr<'column, S, bool> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(move |b| sea_query::Expr::expr(lhs.build_expr(b)).glob(rhs.build_expr(b)))
    }

    /// Check if the expression matches the pattern [sqlite docs](https://www.sqlite.org/lang_expr.html#like).
    ///
    /// As noted in the docs, it is **case-insensitive** for ASCII characters. Other characters are case-sensitive.
    /// For creating patterns it uses `%` as a wildcard for any sequence of characters and `_` for any single character.
    /// Special characters should be escaped with `\`.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one("hello world".into_expr().like("HELLO%")), true);
    /// assert_eq!(txn.query_one("hello world".into_expr().like("he_o%")), false);
    /// # });
    /// ```
    pub fn like(&self, pattern: impl Into<String>) -> Expr<'column, S, bool> {
        let lhs = self.inner.clone();
        let rhs = pattern.into();
        Expr::adhoc(move |b| {
            sea_query::Expr::expr(lhs.build_expr(b))
                .like(sea_query::LikeExpr::new(&rhs).escape('\\'))
        })
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
