pub mod operations;

use std::{marker::PhantomData, ops::Deref, rc::Rc};

use operations::{Add, And, AsFloat, Eq, IsNotNull, Lt, Not, UnwrapOr};
use ref_cast::RefCast;
use rusqlite::types::FromSql;
use sea_query::{Alias, Expr, Nullable, SelectStatement, SimpleExpr};

use crate::{
    alias::{Field, MyAlias, RawAlias},
    ast::{MySelect, Source},
    db::Row,
    hash,
    migrate::NoTable,
    Table,
};

#[derive(Clone, Copy)]
pub struct ValueBuilder<'x> {
    pub(crate) inner: &'x MySelect,
}

impl<'x> ValueBuilder<'x> {
    pub(crate) fn get_aggr(
        self,
        aggr: SelectStatement,
        conds: Vec<(Field, SimpleExpr)>,
    ) -> MyAlias {
        let source = Source {
            kind: crate::ast::SourceKind::Aggregate(aggr),
            conds,
        };
        let new_alias = || self.inner.scope.new_alias();
        *self.inner.extra.get_or_init(source, new_alias)
    }

    pub(crate) fn get_join<T: Table>(self, expr: SimpleExpr) -> MyAlias {
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
impl<T: Table> EqTyp for T {}

/// Typ does not depend on scope, so it gets its own trait
pub trait Typed {
    type Typ;

    #[doc(hidden)]
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr;
    #[doc(hidden)]
    fn build_table(&self, b: crate::value::ValueBuilder) -> MyAlias
    where
        Self::Typ: Table,
    {
        b.get_join::<Self::Typ>(self.build_expr(b))
    }
}

/// Trait for all values that can be used in queries.
/// This includes dummies from queries and rust values.
/// `'t` is the context in which this value is valid.
/// `S` is the schema in which this value is valid.
pub trait Value<'t, S>: Typed + Clone {
    #[doc(hidden)]
    type Owned: Typed<Typ = Self::Typ> + 't;

    #[doc(hidden)]
    fn into_owned(self) -> Self::Owned;

    fn into_dyn(self) -> DynValue<'t, S, Self::Typ> {
        DynValue(Rc::new(self.into_owned()), PhantomData)
    }

    fn add<T: Value<'t, S, Typ = Self::Typ>>(&self, rhs: T) -> Add<Self, T>
    where
        Self::Typ: NumTyp,
    {
        Add(self.clone(), rhs)
    }

    fn lt<T: Value<'t, S, Typ = Self::Typ>>(&self, rhs: T) -> Lt<Self, T>
    where
        Self::Typ: NumTyp,
    {
        Lt(self.clone(), rhs)
    }

    fn eq<T: Value<'t, S, Typ = Self::Typ>>(&self, rhs: T) -> Eq<Self, T>
    where
        Self::Typ: EqTyp,
    {
        Eq(self.clone(), rhs)
    }

    fn not(self) -> Not<Self>
    where
        Self: Value<'t, S, Typ = bool>,
    {
        Not(self.clone())
    }

    fn and<T: Value<'t, S, Typ = bool>>(&self, rhs: T) -> And<Self, T>
    where
        Self: Value<'t, S, Typ = bool>,
    {
        And(self.clone(), rhs)
    }

    fn unwrap_or<T: Value<'t, S>>(&self, rhs: T) -> UnwrapOr<Self, T>
    where
        Self: Value<'t, S, Typ = Option<T::Typ>>,
    {
        UnwrapOr(self.clone(), rhs)
    }

    fn is_not_null<Typ>(&self) -> IsNotNull<Self>
    where
        Self: Value<'t, S, Typ = Option<Typ>>,
    {
        IsNotNull(self.clone())
    }

    fn as_float(&self) -> AsFloat<Self>
    where
        Self: Value<'t, S, Typ = i64>,
    {
        AsFloat(self.clone())
    }
}

impl<T: Typed<Typ = X>, X: MyTyp<Sql: Nullable>> Typed for Option<T> {
    type Typ = Option<T::Typ>;

    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.as_ref()
            .map(|x| T::build_expr(x, b))
            .unwrap_or(X::Sql::null().into())
    }
}

impl<'t, S, T: Value<'t, S, Typ = X>, X: MyTyp<Sql: Nullable>> Value<'t, S> for Option<T> {
    type Owned = Option<T::Owned>;
    fn into_owned(self) -> Self::Owned {
        self.map(Value::into_owned)
    }
}

impl Typed for &str {
    type Typ = String;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

impl<'t, S> Value<'t, S> for &str {
    type Owned = String;
    fn into_owned(self) -> Self::Owned {
        self.to_owned()
    }
}

impl Typed for String {
    type Typ = String;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(self)
    }
}

impl<'t, S> Value<'t, S> for String {
    type Owned = String;
    fn into_owned(self) -> Self::Owned {
        self
    }
}

impl Typed for bool {
    type Typ = bool;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

impl<'t, S> Value<'t, S> for bool {
    type Owned = Self;
    fn into_owned(self) -> Self::Owned {
        self
    }
}

impl Typed for i64 {
    type Typ = i64;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

impl<'t, S> Value<'t, S> for i64 {
    type Owned = Self;
    fn into_owned(self) -> Self::Owned {
        self
    }
}

impl Typed for f64 {
    type Typ = f64;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

impl<'t, S> Value<'t, S> for f64 {
    type Owned = Self;
    fn into_owned(self) -> Self::Owned {
        self
    }
}

impl<T> Typed for &T
where
    T: Typed,
{
    type Typ = T::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        T::build_expr(self, b)
    }
    fn build_table(&self, b: crate::value::ValueBuilder) -> MyAlias
    where
        Self::Typ: Table,
    {
        T::build_table(self, b)
    }
}

impl<'t, S, T> Value<'t, S> for &T
where
    T: Value<'t, S>,
{
    type Owned = T::Owned;
    fn into_owned(self) -> Self::Owned {
        T::into_owned(self.clone())
    }
}

/// Use this a value in a query to get the current datetime as a number.
#[derive(Clone, Copy)]
pub struct UnixEpoch;

impl Typed for UnixEpoch {
    type Typ = i64;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        Expr::col(RawAlias("unixepoch('now')".to_owned())).into()
    }
}

impl<'t, S> Value<'t, S> for UnixEpoch {
    type Owned = Self;
    fn into_owned(self) -> Self::Owned {
        self
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
    type In<'t>;
    #[doc(hidden)]
    type Sql;
}

impl<T: Table> MyTyp for T {
    const TYP: hash::ColumnType = hash::ColumnType::Integer;
    const FK: Option<(&'static str, &'static str)> = Some((T::NAME, T::ID));
    type Out<'t> = Row<'t, Self>;
    type In<'t> = Row<'t, Self>;
    type Sql = i64;
}

impl MyTyp for i64 {
    const TYP: hash::ColumnType = hash::ColumnType::Integer;
    type Out<'t> = Self;
    type In<'t> = Self;
    type Sql = i64;
}

impl MyTyp for f64 {
    const TYP: hash::ColumnType = hash::ColumnType::Float;
    type Out<'t> = Self;
    type In<'t> = Self;
    type Sql = f64;
}

impl MyTyp for bool {
    const TYP: hash::ColumnType = hash::ColumnType::Integer;
    type Out<'t> = Self;
    type In<'t> = Self;
    type Sql = bool;
}

impl MyTyp for String {
    const TYP: hash::ColumnType = hash::ColumnType::String;
    type Out<'t> = Self;
    type In<'t> = &'t str;
    type Sql = String;
}

impl<T: MyTyp> MyTyp for Option<T> {
    const TYP: hash::ColumnType = T::TYP;
    const NULLABLE: bool = true;
    const FK: Option<(&'static str, &'static str)> = T::FK;
    type Out<'t> = Option<T::Out<'t>>;
    type In<'t> = Option<T::In<'t>>;
    type Sql = T::Sql;
}

impl MyTyp for NoTable {
    const TYP: hash::ColumnType = hash::ColumnType::Integer;
    type Out<'t> = NoTable;
    type In<'t> = NoTable;
    type Sql = i64;
}

impl FromSql for NoTable {
    fn column_result(_: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        unreachable!()
    }
}

pub struct DynValue<'t, S, T>(Rc<dyn Typed<Typ = T> + 't>, PhantomData<S>);

impl<'t, S, T> Clone for DynValue<'t, S, T> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), PhantomData)
    }
}

impl<'t, S, T> Typed for DynValue<'t, S, T> {
    type Typ = T;

    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.as_ref().build_expr(b)
    }

    fn build_table(&self, b: crate::value::ValueBuilder) -> MyAlias
    where
        Self::Typ: Table,
    {
        self.0.as_ref().build_table(b)
    }
}

impl<'t, S: 't, T: 't> Value<'t, S> for DynValue<'t, S, T> {
    type Owned = Self;
    fn into_owned(self) -> Self::Owned {
        self
    }
}

impl<'t, S, T: Table> Deref for DynValue<'t, S, T> {
    type Target = T::Ext<Self>;

    fn deref(&self) -> &Self::Target {
        RefCast::ref_cast(self)
    }
}

#[test]
fn lifetimes() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile/*.rs");
}
