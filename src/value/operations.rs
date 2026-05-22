use std::rc::Rc;

use crate::{
    lower::{self, CONST_0, CONST_NULL},
    value::{BuffTyp, OrdTyp},
};

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
    pub fn add(&self, rhs: impl IntoExpr<'column, S, Typ = T>) -> Self {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(lower::Expr::Infix(lhs, "+", rhs))
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
    pub fn sub(&self, rhs: impl IntoExpr<'column, S, Typ = T>) -> Self {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(lower::Expr::Infix(lhs, "-", rhs))
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
    pub fn mul(&self, rhs: impl IntoExpr<'column, S, Typ = T>) -> Self {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(lower::Expr::Infix(lhs, "*", rhs))
    }

    /// Divide one expression by another.
    ///
    /// For integers, the result is truncated towards zero.
    /// See also [Expr::modulo].
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(5.into_expr().div(3)), 1);
    /// assert_eq!(txn.query_one((-5).into_expr().div(3)), -1);
    /// assert_eq!(txn.query_one(1.0.into_expr().div(2.0)), 0.5);
    /// # });
    /// ```
    pub fn div(&self, rhs: impl IntoExpr<'column, S, Typ = T>) -> Self {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(lower::Expr::Infix(lhs, "/", rhs))
    }

    /// Get the sign of the expression.
    ///
    /// The result is -1, 0 or 1 depending on if the expression is
    /// negative, zero or positive.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(2.into_expr().sign()), 1);
    /// assert_eq!(txn.query_one((-5.0).into_expr().sign()), -1);
    /// assert_eq!(txn.query_one((-0.0).into_expr().sign()), 0);
    /// # });
    /// ```
    pub fn sign(&self) -> Expr<'column, S, i64> {
        let lhs = self.inner.clone();
        Expr::adhoc(lower::Expr::Func("sign", Box::new([lhs])))
    }

    /// Get the absolute value of the expression.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(2.into_expr().abs()), 2);
    /// assert_eq!(txn.query_one((-5.0).into_expr().abs()), 5.0);
    /// # });
    /// ```
    pub fn abs(&self) -> Self {
        let lhs = self.inner.clone();
        Expr::adhoc(lower::Expr::Func("abs", Box::new([lhs])))
    }
}

impl<'column, S, T: OrdTyp> Expr<'column, S, T> {
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
        Expr::adhoc(lower::Expr::Infix(lhs, "<", rhs))
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
        Expr::adhoc(lower::Expr::Infix(lhs, "<=", rhs))
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
        Expr::adhoc(lower::Expr::Infix(lhs, ">", rhs))
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
        Expr::adhoc(lower::Expr::Infix(lhs, ">=", rhs))
    }

    /// Get the maximum of two values.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(2.into_expr().max(3)), 3);
    /// assert_eq!(txn.query_one(5.0.into_expr().max(3.0)), 5.0);
    /// # });
    /// ```
    pub fn max(&self, rhs: impl IntoExpr<'column, S, Typ = T>) -> Self {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(lower::Expr::Func("max", Box::new([lhs, rhs])))
    }

    /// Get the minimum of two values.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(2.into_expr().min(3)), 2);
    /// assert_eq!(txn.query_one(5.0.into_expr().min(3.0)), 3.0);
    /// # });
    /// ```
    pub fn min(&self, rhs: impl IntoExpr<'column, S, Typ = T>) -> Self {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(lower::Expr::Func("min", Box::new([lhs, rhs])))
    }

    /// Check if a value is between two other values.
    ///
    /// The range is inclusive on both sides.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(2.into_expr().between(2, 3)), true);
    /// assert_eq!(txn.query_one(3.into_expr().between(2, 3)), true);
    /// assert_eq!(txn.query_one(5.into_expr().between(2, 3)), false);
    /// assert_eq!(txn.query_one(1.into_expr().between(2, 3)), false);
    /// # });
    /// ```
    pub fn between(
        &self,
        low: impl IntoExpr<'column, S, Typ = T>,
        high: impl IntoExpr<'column, S, Typ = T>,
    ) -> Expr<'column, S, bool> {
        let lhs = self.inner.clone();
        let low = low.into_expr().inner;
        let high = high.into_expr().inner;
        Expr::adhoc(lower::Expr::Between(lhs, low, high))
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
        Expr::adhoc(lower::Expr::Infix(lhs, "IS", rhs))
    }

    /// Check whether two expressions are not equal.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(2.into_expr().neq(2)), false);
    /// assert_eq!(txn.query_one(3.0.into_expr().neq(3.1)), true);
    /// assert_eq!(txn.query_one("test".into_expr().neq("test")), false);
    /// assert_eq!(txn.query_one(b"test".into_expr().neq(b"test" as &[u8])), false);
    /// assert_eq!(txn.query_one(false.into_expr().neq(false)), false);
    ///
    /// assert_eq!(txn.query_one(1.into_expr().neq(2)), true);
    /// # });
    /// ```
    pub fn neq(&self, rhs: impl IntoExpr<'column, S, Typ = T>) -> Expr<'column, S, bool> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(lower::Expr::Infix(lhs, "IS NOT", rhs))
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
    pub fn not(&self) -> Self {
        let val = self.inner.clone();
        Expr::adhoc(lower::Expr::Prefix("~", val))
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
    pub fn and(&self, rhs: impl IntoExpr<'column, S, Typ = bool>) -> Self {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(lower::Expr::Infix(lhs, "AND", rhs))
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
    pub fn or(&self, rhs: impl IntoExpr<'column, S, Typ = bool>) -> Self {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(lower::Expr::Infix(lhs, "OR", rhs))
    }
}

impl<'column, S, Typ: EqTyp> Expr<'column, S, Option<Typ>> {
    /// Use the first expression if it is [Some], otherwise use the second expression.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(Some(10).into_expr().unwrap_or(5)), 10);
    /// assert_eq!(txn.query_one(None::<String>.into_expr().unwrap_or("foo")), "foo");
    /// # });
    /// ```
    pub fn unwrap_or(&self, rhs: impl IntoExpr<'column, S, Typ = Typ>) -> Expr<'column, S, Typ> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(lower::Expr::Func("ifnull", Box::new([lhs, rhs])))
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
        Expr::adhoc(lower::Expr::Infix(val, "IS NOT", Rc::new(CONST_NULL)))
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
        Expr::adhoc(lower::Expr::Infix(val, "IS", Rc::new(CONST_NULL)))
    }
}

impl<'column, S> Expr<'column, S, i64> {
    /// Convert the [i64] expression to [f64] type.
    ///
    /// The conversion may not be lossless for integers larger than 2^53 or smaller than (-2^53).
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(10.into_expr().to_f64()), 10.0);
    /// # });
    /// ```
    pub fn to_f64(&self) -> Expr<'column, S, f64> {
        let val = self.inner.clone();
        Expr::adhoc(lower::Expr::Cast(val, "REAL"))
    }

    /// Calculate the remainder for integer division.
    ///
    /// The remainder is the missing part after division.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(5.into_expr().div(3)), 1);
    /// assert_eq!(txn.query_one(5.into_expr().modulo(3)), 2);
    /// assert_eq!(txn.query_one((-5).into_expr().div(3)), -1);
    /// assert_eq!(txn.query_one((-5).into_expr().modulo(3)), -2);
    /// # });
    /// ```
    pub fn modulo(&self, rhs: impl IntoExpr<'column, S, Typ = i64>) -> Self {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(lower::Expr::Infix(lhs, "%", rhs))
    }
}

impl<'column, S> Expr<'column, S, f64> {
    /// Convert the [f64] expression to [i64] type.
    ///
    /// Always rounds towards zero for floats that are not already an integer.
    ///
    /// Values outside the range `i64::MIN..=i64::MAX`, will be converted to
    /// the closest integer in that range.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(10.9.into_expr().to_i64()), 10);
    /// assert_eq!(txn.query_one((-10.9).into_expr().to_i64()), -10);
    /// assert_eq!(txn.query_one((342143124.0).into_expr().to_i64()), 342143124);
    /// assert_eq!(txn.query_one((f64::MIN).into_expr().to_i64()), i64::MIN);
    /// assert_eq!(txn.query_one((f64::NEG_INFINITY).into_expr().to_i64()), i64::MIN);
    /// # });
    /// ```
    pub fn to_i64(&self) -> Expr<'column, S, i64> {
        let val = self.inner.clone();
        Expr::adhoc(lower::Expr::Cast(val, "INTEGER"))
    }

    /// Round the [f64] expression down.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(10.9.into_expr().floor()), 10.0);
    /// assert_eq!(txn.query_one((-10.9).into_expr().floor()), -11.0);
    /// assert_eq!(txn.query_one((f64::MIN).into_expr().floor()), f64::MIN);
    /// assert_eq!(txn.query_one((f64::NEG_INFINITY).into_expr().floor()), f64::NEG_INFINITY);
    /// # });
    /// ```
    pub fn floor(&self) -> Expr<'column, S, f64> {
        let val = self.inner.clone();
        Expr::adhoc(lower::Expr::Func("floor", Box::new([val])))
    }

    /// Round the [f64] expression up.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(10.9.into_expr().ceil()), 11.0);
    /// assert_eq!(txn.query_one((-10.9).into_expr().ceil()), -10.0);
    /// assert_eq!(txn.query_one((f64::MIN).into_expr().ceil()), f64::MIN);
    /// assert_eq!(txn.query_one((f64::NEG_INFINITY).into_expr().ceil()), f64::NEG_INFINITY);
    /// # });
    /// ```
    pub fn ceil(&self) -> Expr<'column, S, f64> {
        let val = self.inner.clone();
        Expr::adhoc(lower::Expr::Func("ceil", Box::new([val])))
    }

    /// Round the [f64] expression to the specified precision.
    ///
    /// Precision specifies the number of decimal places to keep.
    /// A negative precision is treated as zero.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(10.85.into_expr().round_with_precision(0)), 11.0);
    /// assert_eq!(txn.query_one((-10.85).into_expr().round_with_precision(0)), -11.0);
    /// assert_eq!(txn.query_one(10.85.into_expr().round_with_precision(1)), 10.8);
    /// assert_eq!(txn.query_one((-10.85).into_expr().round_with_precision(1)), -10.8);
    /// assert_eq!(txn.query_one((f64::MIN).into_expr().round_with_precision(0)), f64::MIN);
    /// assert_eq!(txn.query_one((f64::NEG_INFINITY).into_expr().round_with_precision(0)), f64::NEG_INFINITY);
    /// # });
    /// ```
    pub fn round_with_precision(&self, precision: impl IntoExpr<'column, S, Typ = i64>) -> Self {
        let val = self.inner.clone();
        let precision = precision.into_expr().inner;
        Expr::adhoc(lower::Expr::Func("round", Box::new([val, precision])))
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
    #[doc(alias = "instr")]
    pub fn contains(&self, rhs: impl IntoExpr<'column, S, Typ = String>) -> Expr<'column, S, bool> {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(lower::Expr::Infix(
            Rc::new(lower::Expr::Func("instr", Box::new([lhs, rhs]))),
            "IS NOT",
            Rc::new(CONST_0),
        ))
    }

    /// Replace all occurences of a string with another string.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one("very,cool,list".into_expr().replace(",", "::")), "very::cool::list");
    /// assert_eq!(txn.query_one("rarar".into_expr().replace("rar", "rer")), "rerar");
    /// # });
    /// ```
    pub fn replace(
        &self,
        pattern: impl IntoExpr<'column, S, Typ = String>,
        new: impl IntoExpr<'column, S, Typ = String>,
    ) -> Self {
        let lhs = self.inner.clone();
        let pat = pattern.into_expr().inner;
        let new = new.into_expr().inner;
        Expr::adhoc(lower::Expr::Func("replace", Box::new([lhs, pat, new])))
    }

    /// Removes all characters from a set from the start of a string.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one("abacda".into_expr().ltrim("ab")), "cda");
    /// # });
    /// ```
    pub fn ltrim(&self, char_set: impl IntoExpr<'column, S, Typ = String>) -> Self {
        let lhs = self.inner.clone();
        let char_set = char_set.into_expr().inner;
        Expr::adhoc(lower::Expr::Func("ltrim", Box::new([lhs, char_set])))
    }

    /// Removes all characters from a set from the end of a string.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one("adcaba".into_expr().rtrim("ab")), "adc");
    /// # });
    /// ```
    pub fn rtrim(&self, char_set: impl IntoExpr<'column, S, Typ = String>) -> Self {
        let lhs = self.inner.clone();
        let char_set = char_set.into_expr().inner;
        Expr::adhoc(lower::Expr::Func("rtrim", Box::new([lhs, char_set])))
    }

    /// Removes all characters from a set from both the start and end of a string.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one("ffzfzf".into_expr().trim("f")), "zfz");
    /// # });
    /// ```
    pub fn trim(&self, char_set: impl IntoExpr<'column, S, Typ = String>) -> Self {
        let lhs = self.inner.clone();
        let char_set = char_set.into_expr().inner;
        Expr::adhoc(lower::Expr::Func("trim", Box::new([lhs, char_set])))
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
        Expr::adhoc(lower::Expr::Func("glob", Box::new([lhs, rhs])))
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
        let rhs: Expr<S, _> = pattern.into().into_expr();
        let rhs = rhs.inner;
        Expr::adhoc(lower::Expr::Func(
            "like",
            Box::new([rhs, lhs, Rc::new(lower::Expr::Constant("'\\'"))]),
        ))
    }

    /// Concatenate two strings.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one("hello ".into_expr().concat("world").concat("!")), "hello world!");
    /// # });
    /// ```
    pub fn concat(&self, rhs: impl IntoExpr<'column, S, Typ = String>) -> Self {
        let lhs = self.inner.clone();
        let rhs = rhs.into_expr().inner;
        Expr::adhoc(lower::Expr::Infix(lhs, "||", rhs))
    }

    /// Convert ascii to lowercase.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one("Hello".into_expr().lower()), "hello");
    /// assert_eq!(txn.query_one("WHAT".into_expr().lower()), "what");
    /// # });
    /// ```
    pub fn lower(&self) -> Self {
        let lhs = self.inner.clone();
        Expr::adhoc(lower::Expr::Func("lower", Box::new([lhs])))
    }

    /// Convert ascii to uppercase.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one("Hello".into_expr().upper()), "HELLO");
    /// assert_eq!(txn.query_one("what".into_expr().upper()), "WHAT");
    /// # });
    /// ```
    pub fn upper(&self) -> Self {
        let lhs = self.inner.clone();
        Expr::adhoc(lower::Expr::Func("upper", Box::new([lhs])))
    }

    /// The number of unicode code points in the string.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one("€".into_expr().char_len()), 1);
    /// assert_eq!(txn.query_one("what".into_expr().char_len()), 4);
    /// # });
    /// ```
    pub fn char_len(&self) -> Expr<'column, S, i64> {
        let lhs = self.inner.clone();
        Expr::adhoc(lower::Expr::Func("length", Box::new([lhs])))
    }
}

impl<'column, S, T: BuffTyp> Expr<'column, S, T> {
    /// The length of the value in bytes.
    ///
    /// The byte length of strings can depend on the encoding (UTF-8 or UTF-16).
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one("€".into_expr().byte_len()), 3);
    /// assert_eq!(txn.query_one("what".into_expr().byte_len()), 4);
    /// assert_eq!(txn.query_one(vec![1, 2].into_expr().byte_len()), 2);
    /// # });
    /// ```
    pub fn byte_len(&self) -> Expr<'column, S, i64> {
        let lhs = self.inner.clone();
        Expr::adhoc(lower::Expr::Func("octet_length", Box::new([lhs])))
    }
}

impl<'column, S> Expr<'column, S, Vec<u8>> {
    /// Create a new blob of zero bytes of the specified length.
    ///
    /// ```
    /// # use rust_query::Expr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// assert_eq!(txn.query_one(Expr::zero_blob(40)), vec![0; 40]);
    /// # });
    /// ```
    pub fn zero_blob(len: impl IntoExpr<'column, S, Typ = i64>) -> Self {
        let len = len.into_expr().inner;
        Expr::adhoc(lower::Expr::Func("zeroblov", Box::new([len])))
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
