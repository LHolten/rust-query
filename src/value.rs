use std::marker::PhantomData;

use rusqlite::types::FromSql;
use sea_query::{Alias, Expr, Nullable, SimpleExpr};

use crate::{
    alias::{MyAlias, RawAlias},
    ast::{MySelect, Source},
    db::{Col, Db, Just},
    hash, HasId, NoTable,
};

#[derive(Clone, Copy)]
pub struct ValueBuilder<'x> {
    pub(crate) inner: &'x MySelect,
}

impl<'x> ValueBuilder<'x> {
    pub(crate) fn get_join<T: HasId>(self, expr: SimpleExpr) -> MyAlias {
        let source = Source::Implicit {
            table: T::NAME.to_owned(),
            conds: vec![(T::ID, expr)],
        };
        *self.inner.extra.get_or_init(source, MyAlias::new)
    }

    pub fn get_unique(
        self,
        table: &'static str,
        conds: Vec<(&'static str, SimpleExpr)>,
    ) -> SimpleExpr {
        let source = Source::Implicit {
            table: table.to_owned(),
            conds,
        };
        let table = self.inner.extra.get_or_init(source, MyAlias::new);
        Expr::col((*table, Alias::new("id"))).into()
    }
}

/// Trait for all values that can be used in queries.
/// This includes dummies from queries and rust values.
pub trait Value<'t>: Sized + Clone {
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
/// [Covariant]`<'t>` can be implemented if [Value]`<'a>` is implemented for all `'a` shorter or equal to `'t`.
pub trait Covariant<'t>: Value<'t> {}

impl<'t, T, P: Value<'t, Typ: HasId>> Value<'t> for Col<T, P> {
    type Typ = T;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        let table = b.get_join::<P::Typ>(self.inner.build_expr(b));
        Expr::col((table, self.field)).into()
    }
}
impl<'t, T, P: Covariant<'t, Typ: HasId>> Covariant<'t> for Col<T, P> {}

impl<'t, T, X> Value<'t> for Col<T, Db<'t, X>> {
    type Typ = T;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        Expr::col((self.inner.table, self.field)).into()
    }
}

impl<'t, T> Value<'t> for Just<'t, T> {
    type Typ = T;

    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        Expr::val(self.idx).into()
    }
}
impl<'t, T> Covariant<'t> for Just<'t, T> {}

impl<'t, T: Value<'t>> Value<'t> for &'_ T {
    type Typ = T::Typ;

    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        T::build_expr(self, b)
    }
}
impl<'t, T: Covariant<'t>> Covariant<'t> for &'_ T {}

impl<'t, T: Value<'t, Typ = X>, X: MyTyp<Sql: Nullable>> Value<'t> for Option<T> {
    type Typ = Option<T::Typ>;

    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.as_ref()
            .map(|x| T::build_expr(x, b))
            .unwrap_or(X::Sql::null().into())
    }
}
impl<'t, T: Covariant<'t, Typ = X>, X: MyTyp<Sql: Nullable>> Covariant<'t> for Option<T> {}

impl<'t> Value<'t> for &str {
    type Typ = String;

    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}
impl<'t> Covariant<'t> for &str {}

impl<'t> Value<'t> for String {
    type Typ = String;

    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(self)
    }
}
impl<'t> Covariant<'t> for String {}

impl<'t> Value<'t> for bool {
    type Typ = bool;

    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}
impl<'t> Covariant<'t> for bool {}

impl<'t> Value<'t> for i64 {
    type Typ = i64;

    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}
impl<'t> Covariant<'t> for i64 {}

impl<'t> Value<'t> for f64 {
    type Typ = f64;

    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}
impl<'t> Covariant<'t> for f64 {}

#[derive(Clone, Copy)]
pub struct MyAdd<A, B>(A, B);

impl<'t, A: Value<'t>, B: Value<'t>> Value<'t> for MyAdd<A, B> {
    type Typ = A::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).add(self.1.build_expr(b))
    }
}
impl<'t, A: Covariant<'t>, B: Covariant<'t>> Covariant<'t> for MyAdd<A, B> {}

#[derive(Clone, Copy)]
pub struct MyNot<T>(T);

impl<'t, T: Value<'t>> Value<'t> for MyNot<T> {
    type Typ = T::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).not()
    }
}
impl<'t, T: Covariant<'t>> Covariant<'t> for MyNot<T> {}

#[derive(Clone, Copy)]
pub struct MyAnd<A, B>(A, B);

impl<'t, A: Value<'t>, B: Value<'t>> Value<'t> for MyAnd<A, B> {
    type Typ = A::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).and(self.1.build_expr(b))
    }
}
impl<'t, A: Covariant<'t>, B: Covariant<'t>> Covariant<'t> for MyAnd<A, B> {}

#[derive(Clone, Copy)]
pub struct MyLt<A>(A, i32);

impl<'t, A: Value<'t>> Value<'t> for MyLt<A> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).lt(self.1)
    }
}
impl<'t, A: Covariant<'t>> Covariant<'t> for MyLt<A> {}

#[derive(Clone, Copy)]
pub struct MyEq<A, B>(A, B);

impl<'t, A: Value<'t>, B: Value<'t>> Value<'t> for MyEq<A, B> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).eq(self.1.build_expr(b))
    }
}
impl<'t, A: Covariant<'t>, B: Covariant<'t>> Covariant<'t> for MyEq<A, B> {}

#[derive(Clone, Copy)]
pub struct UnwrapOr<A, B>(pub(crate) A, pub(crate) B);

impl<'t, A: Value<'t>, B: Value<'t>> Value<'t> for UnwrapOr<A, B> {
    type Typ = B::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).if_null(self.1.build_expr(b))
    }
}
impl<'t, A: Covariant<'t>, B: Covariant<'t>> Covariant<'t> for UnwrapOr<A, B> {}

#[derive(Clone, Copy)]
pub struct IsNotNull<A>(pub(crate) A);

impl<'t, A: Value<'t>> Value<'t> for IsNotNull<A> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).is_not_null()
    }
}
impl<'t, A: Covariant<'t>> Covariant<'t> for IsNotNull<A> {}

#[derive(Clone, Copy)]
pub struct Assume<A>(pub(crate) A);

impl<'t, T, A: Value<'t, Typ = Option<T>>> Value<'t> for Assume<A> {
    type Typ = T;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b)
    }
}
impl<'t, T, A: Covariant<'t, Typ = Option<T>>> Covariant<'t> for Assume<A> {}

/// Use this a value in a query to get the current datetime as a number.
#[derive(Clone)]
pub struct UnixEpoch;

impl<'t> Value<'t> for UnixEpoch {
    type Typ = i64;

    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        Expr::col(RawAlias("unixepoch('now')".to_owned())).into()
    }
}
impl<'t> Covariant<'t> for UnixEpoch {}

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

impl<'t, T> FromSql for Just<'t, T> {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(Self {
            _p: PhantomData,
            idx: value.as_i64()?,
        })
    }
}

impl<'t, T> From<Just<'t, T>> for sea_query::Value {
    fn from(value: Just<T>) -> Self {
        value.idx.into()
    }
}

#[test]
fn lifetimes() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile/*.rs");
}
