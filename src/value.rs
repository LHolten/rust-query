pub mod operations;

use std::rc::Rc;

use operations::{Add, And, AsFloat, Eq, IsNotNull, Lt, Not, UnwrapOr};
use rusqlite::types::FromSql;
use sea_query::{Alias, Expr, Nullable, SimpleExpr};

use crate::{
    alias::{Field, MyAlias, RawAlias},
    ast::{MySelect, Source},
    db::Row,
    hash,
    migrate::NoTable,
    HasId,
};

#[derive(Clone, Copy)]
pub struct ValueBuilder<'x> {
    pub(crate) inner: &'x MySelect,
}

impl<'x> ValueBuilder<'x> {
    pub(crate) fn get_join<T: HasId>(self, expr: SimpleExpr) -> MyAlias {
        let source = Source {
            kind: crate::ast::SourceKind::Implicit(T::NAME.to_owned()),
            conds: vec![(Field::Str(T::ID), expr)],
        };
        let new_alias = || self.inner.scope.new_alias();
        *self.inner.extra.get_or_init(source, new_alias)
    }

    pub fn get_unique(
        self,
        table: &'static str,
        conds: Vec<(&'static str, SimpleExpr)>,
    ) -> SimpleExpr {
        let source = Source {
            kind: crate::ast::SourceKind::Implicit(table.to_owned()),
            conds: conds.into_iter().map(|x| (Field::Str(x.0), x.1)).collect(),
        };

        let new_alias = || self.inner.scope.new_alias();
        let table = self.inner.extra.get_or_init(source, new_alias);
        Expr::col((*table, Alias::new("id"))).into()
    }
}

pub trait NumTyp: MyTyp + Clone + Copy {
    const ZERO: Self;
    fn into_value(self) -> sea_query::Value;
}

impl NumTyp for i64 {
    const ZERO: Self = 0;
    fn into_value(self) -> sea_query::Value {
        sea_query::Value::BigInt(Some(self))
    }
}
impl NumTyp for f64 {
    const ZERO: Self = 0.;
    fn into_value(self) -> sea_query::Value {
        sea_query::Value::Double(Some(self))
    }
}

pub trait EqTyp {}

impl EqTyp for String {}
impl EqTyp for i64 {}
impl EqTyp for f64 {}
impl EqTyp for bool {}
impl<T: HasId> EqTyp for T {}

// This prevents implementing `Value<S>` downstream on upstream types with a downstream `S`.
pub trait NoParam {}

/// Trait for all values that can be used in queries.
/// This includes dummies from queries and rust values.
/// `'t` is the context in which this value is valid.
/// `S` is the schema in which this value is valid.
pub trait Value<'t, S>: NoParam {
    type Typ;

    #[doc(hidden)]
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr;

    fn add<T: Value<'t, S, Typ = Self::Typ>>(&self, rhs: T) -> Add<Self, T>
    where
        Self::Typ: NumTyp,
        Self: Clone,
    {
        Add(self.clone(), rhs)
    }

    fn lt<T: Value<'t, S, Typ = Self::Typ>>(&self, rhs: T) -> Lt<Self, T>
    where
        Self::Typ: NumTyp,
        Self: Clone,
    {
        Lt(self.clone(), rhs)
    }

    fn eq<T: Value<'t, S, Typ = Self::Typ>>(&self, rhs: T) -> Eq<Self, T>
    where
        Self::Typ: EqTyp,
        Self: Clone,
    {
        Eq(self.clone(), rhs)
    }

    fn not(self) -> Not<Self>
    where
        Self: Value<'t, S, Typ = bool>,
        Self: Clone,
    {
        Not(self.clone())
    }

    fn and<T: Value<'t, S, Typ = bool>>(&self, rhs: T) -> And<Self, T>
    where
        Self: Value<'t, S, Typ = bool>,
        Self: Clone,
    {
        And(self.clone(), rhs)
    }

    fn unwrap_or<T: Value<'t, S>>(&self, rhs: T) -> UnwrapOr<Self, T>
    where
        Self: Value<'t, S, Typ = Option<T::Typ>>,
        Self: Clone,
    {
        UnwrapOr(self.clone(), rhs)
    }

    fn is_not_null<Typ>(&self) -> IsNotNull<Self>
    where
        Self: Value<'t, S, Typ = Option<Typ>>,
        Self: Clone,
    {
        IsNotNull(self.clone())
    }

    fn as_float(&self) -> AsFloat<Self>
    where
        Self: Value<'t, S, Typ = i64>,
        Self: Clone,
    {
        AsFloat(self.clone())
    }

    fn into_dyn<'a>(self) -> Rc<dyn Value<'t, S, Typ = Self::Typ> + 'a>
    where
        Self: Sized + 'a,
    {
        Rc::new(self)
    }
}

impl<T: NoParam> NoParam for Option<T> {}

impl<'t, S, T: Value<'t, S, Typ = X>, X: MyTyp<Sql: Nullable>> Value<'t, S> for Option<T> {
    type Typ = Option<T::Typ>;

    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.as_ref()
            .map(|x| T::build_expr(x, b))
            .unwrap_or(X::Sql::null().into())
    }
}

impl NoParam for &str {}

impl<'t, S> Value<'t, S> for &str {
    type Typ = String;

    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

impl NoParam for String {}

impl<'t, S> Value<'t, S> for String {
    type Typ = String;

    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(self)
    }
}

impl NoParam for bool {}

impl<'t, S> Value<'t, S> for bool {
    type Typ = bool;

    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

impl NoParam for i64 {}

impl<'t, S> Value<'t, S> for i64 {
    type Typ = i64;

    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

impl NoParam for f64 {}

impl<'t, S> Value<'t, S> for f64 {
    type Typ = f64;

    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

impl<T: NoParam> NoParam for Rc<T> {}

impl<'t, S, T: Value<'t, S>> Value<'t, S> for Rc<T> {
    type Typ = T::Typ;

    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.as_ref().build_expr(b)
    }
}

/// Use this a value in a query to get the current datetime as a number.
#[derive(Clone)]
pub struct UnixEpoch;

impl NoParam for UnixEpoch {}

impl<'t, S> Value<'t, S> for UnixEpoch {
    type Typ = i64;

    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        Expr::col(RawAlias("unixepoch('now')".to_owned())).into()
    }
}

pub trait MyTyp: 'static {
    #[doc(hidden)]
    const NULLABLE: bool = false;
    #[doc(hidden)]
    const TYP: hash::ColumnType;
    #[doc(hidden)]
    const FK: Option<(&'static str, &'static str)> = None;
    #[doc(hidden)]
    type Out<'t>: FromSql;
    #[doc(hidden)]
    type Sql;
}

impl<T: HasId> MyTyp for T {
    const TYP: hash::ColumnType = hash::ColumnType::Integer;
    const FK: Option<(&'static str, &'static str)> = Some((T::NAME, T::ID));
    type Out<'t> = Row<'t, Self>;
    type Sql = i64;
}

impl MyTyp for i64 {
    const TYP: hash::ColumnType = hash::ColumnType::Integer;
    type Out<'t> = Self;
    type Sql = i64;
}

impl MyTyp for f64 {
    const TYP: hash::ColumnType = hash::ColumnType::Float;
    type Out<'t> = Self;
    type Sql = f64;
}

impl MyTyp for bool {
    const TYP: hash::ColumnType = hash::ColumnType::Integer;
    type Out<'t> = Self;
    type Sql = bool;
}

impl MyTyp for String {
    const TYP: hash::ColumnType = hash::ColumnType::String;
    type Out<'t> = Self;
    type Sql = String;
}

impl<T: MyTyp> MyTyp for Option<T> {
    const TYP: hash::ColumnType = T::TYP;
    const NULLABLE: bool = true;
    const FK: Option<(&'static str, &'static str)> = T::FK;
    type Out<'t> = Option<T::Out<'t>>;
    type Sql = T::Sql;
}

impl MyTyp for NoTable {
    const TYP: hash::ColumnType = hash::ColumnType::Integer;
    type Out<'t> = Row<'t, Self>;
    type Sql = i64;
}

#[test]
fn lifetimes() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile/*.rs");
}
