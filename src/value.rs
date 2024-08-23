use rusqlite::types::FromSql;
use sea_query::{Alias, Expr, Nullable, SimpleExpr};

use crate::{
    alias::{Field, MyAlias, RawAlias},
    ast::{MySelect, Source},
    db::Just,
    hash, HasId, NoTable,
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

/// Trait for all values that can be used in queries.
/// This includes dummies from queries and rust values.
pub trait Value<'t>: Clone {
    type Typ;

    #[doc(hidden)]
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr;

    fn add<T: Value<'t>>(self, rhs: T) -> MyAdd<Self, T> {
        MyAdd(self, rhs)
    }

    fn lt(self, rhs: i32) -> MyLt<Self>
    where
        Self: Value<'t, Typ = i64>,
    {
        MyLt(self, rhs)
    }

    fn eq<T: Value<'t, Typ = Self::Typ>>(self, rhs: T) -> MyEq<Self, T> {
        MyEq(self, rhs)
    }

    fn not(self) -> MyNot<Self>
    where
        Self: Value<'t, Typ = bool>,
    {
        MyNot(self)
    }

    fn and<T: Value<'t, Typ = bool>>(self, rhs: T) -> MyAnd<Self, T>
    where
        Self: Value<'t, Typ = bool>,
    {
        MyAnd(self, rhs)
    }

    fn unwrap_or<T: Value<'t>>(self, rhs: T) -> UnwrapOr<Self, T>
    where
        Self: Value<'t, Typ = Option<T::Typ>>,
    {
        UnwrapOr(self, rhs)
    }

    #[allow(clippy::wrong_self_convention)]
    fn is_not_null(self) -> IsNotNull<Self> {
        IsNotNull(self)
    }
}

impl<'t, T: Value<'t>> Value<'t> for &'_ T {
    type Typ = T::Typ;

    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        T::build_expr(self, b)
    }
}

impl<'t, T: Value<'t, Typ = X>, X: MyTyp<Sql: Nullable>> Value<'t> for Option<T> {
    type Typ = Option<T::Typ>;

    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.as_ref()
            .map(|x| T::build_expr(x, b))
            .unwrap_or(X::Sql::null().into())
    }
}

impl<'t> Value<'t> for &str {
    type Typ = String;

    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

impl<'t> Value<'t> for String {
    type Typ = String;

    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(self)
    }
}

impl<'t> Value<'t> for bool {
    type Typ = bool;

    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

impl<'t> Value<'t> for i64 {
    type Typ = i64;

    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

impl<'t> Value<'t> for f64 {
    type Typ = f64;

    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

#[derive(Clone, Copy)]
pub struct MyAdd<A, B>(A, B);

impl<'t, A: Value<'t>, B: Value<'t>> Value<'t> for MyAdd<A, B> {
    type Typ = A::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).add(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct MyNot<T>(T);

impl<'t, T: Value<'t>> Value<'t> for MyNot<T> {
    type Typ = T::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).not()
    }
}

#[derive(Clone, Copy)]
pub struct MyAnd<A, B>(A, B);

impl<'t, A: Value<'t>, B: Value<'t>> Value<'t> for MyAnd<A, B> {
    type Typ = A::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).and(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct MyLt<A>(A, i32);

impl<'t, A: Value<'t>> Value<'t> for MyLt<A> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).lt(self.1)
    }
}

#[derive(Clone, Copy)]
pub struct MyEq<A, B>(A, B);

impl<'t, A: Value<'t>, B: Value<'t>> Value<'t> for MyEq<A, B> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).eq(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct UnwrapOr<A, B>(pub(crate) A, pub(crate) B);

impl<'t, A: Value<'t>, B: Value<'t>> Value<'t> for UnwrapOr<A, B> {
    type Typ = B::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).if_null(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct IsNotNull<A>(pub(crate) A);

impl<'t, A: Value<'t>> Value<'t> for IsNotNull<A> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).is_not_null()
    }
}

#[derive(Clone, Copy)]
pub struct Assume<A>(pub(crate) A);

impl<'t, T, A: Value<'t, Typ = Option<T>>> Value<'t> for Assume<A> {
    type Typ = T;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b)
    }
}

/// Use this a value in a query to get the current datetime as a number.
#[derive(Clone)]
pub struct UnixEpoch;

impl<'t> Value<'t> for UnixEpoch {
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
    type Out<'t> = Just<'t, Self>;
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
    type Out<'t> = Just<'t, Self>;
    type Sql = i64;
}

#[test]
fn lifetimes() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile/*.rs");
}
