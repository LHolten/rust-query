pub mod operations;

use std::{marker::PhantomData, ops::Deref, rc::Rc};

use operations::{Add, And, AsFloat, Eq, IsNotNull, Like, Lt, Not, NotLike, Or, UnwrapOr};
use ref_cast::RefCast;
use rusqlite::types::FromSql;
use sea_query::{Alias, Expr, Nullable, SelectStatement, SimpleExpr};

use crate::{
    alias::{Field, MyAlias, RawAlias},
    ast::{MySelect, Source},
    db::TableRow,
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
    fn into_sea_value(self) -> sea_query::Value;
}

impl NumTyp for i64 {
    const ZERO: Self = 0;
    fn into_sea_value(self) -> sea_query::Value {
        sea_query::Value::BigInt(Some(self))
    }
}
impl NumTyp for f64 {
    const ZERO: Self = 0.;
    fn into_sea_value(self) -> sea_query::Value {
        sea_query::Value::Double(Some(self))
    }
}

pub trait EqTyp {}

impl EqTyp for String {}
impl EqTyp for i64 {}
impl EqTyp for f64 {}
impl EqTyp for bool {}
impl<T: Table> EqTyp for T {}

pub trait LikeTyp {}

impl LikeTyp for String {}

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
///
/// This includes [Column]s from queries and rust values.
/// - `'t` is the context in which this value is valid.
/// - `S` is the schema in which this value is valid.
///
/// **You can not (yet) implement this trait yourself!**
pub trait IntoColumn<'t, S>: Typed + Clone {
    #[doc(hidden)]
    type Owned: Typed<Typ = Self::Typ> + 't;

    #[doc(hidden)]
    fn into_owned(self) -> Self::Owned;

    /// Turn this value into a [Column].
    fn into_column(self) -> Column<'t, S, Self::Typ> {
        Column(Rc::new(self.into_owned()), PhantomData)
    }
}

impl<'t, S, T: NumTyp> Column<'t, S, T> {
    /// Add two columns together.
    pub fn add(&self, rhs: impl IntoColumn<'t, S, Typ = T>) -> Column<'t, S, T> {
        Add(self, rhs).into_column()
    }

    /// Compute the less than operator of two columns.
    pub fn lt(&self, rhs: impl IntoColumn<'t, S, Typ = T>) -> Column<'t, S, bool> {
        Lt(self, rhs).into_column()
    }
}

impl<'t, S, T: EqTyp + 't> Column<'t, S, T> {
    /// Check whether two columns are equal.
    pub fn eq(&self, rhs: impl IntoColumn<'t, S, Typ = T>) -> Column<'t, S, bool> {
        Eq(self, rhs).into_column()
    }
}

impl<'t, S> Column<'t, S, bool> {
    /// Checks whether a column is false.
    pub fn not(&self) -> Column<'t, S, bool> {
        Not(self).into_column()
    }

    /// Check if two columns are both true.
    pub fn and(&self, rhs: impl IntoColumn<'t, S, Typ = bool>) -> Column<'t, S, bool> {
        And(self, rhs).into_column()
    }

    /// Check if one of two columns is true.
    pub fn or(&self, rhs: impl IntoColumn<'t, S, Typ = bool>) -> Column<'t, S, bool> {
        Or(self, rhs).into_column()
    }
}

impl<'t, S, Typ: 't> Column<'t, S, Option<Typ>> {
    /// Use the first column if it is [Some], otherwise use the second column.
    pub fn unwrap_or(&self, rhs: impl IntoColumn<'t, S, Typ = Typ>) -> Column<'t, S, Typ>
    where
        Self: IntoColumn<'t, S, Typ = Option<Typ>>,
    {
        UnwrapOr(self, rhs).into_column()
    }

    /// Check that the column is [Some].
    pub fn is_some(&self) -> Column<'t, S, bool> {
        IsNotNull(self).into_column()
    }
}

impl<'t, S> Column<'t, S, i64> {
    /// Convert the [i64] column to [f64] type.
    pub fn as_float(&self) -> Column<'t, S, f64> {
        AsFloat(self).into_column()
    }
}

impl<'t, S, T: LikeTyp + 't> Column<'t, S, T> {
    /// Check if the column starts with a string pattern.
    pub fn starts_with(&self, pattern: impl Into<String>) -> Column<'t, S, bool> {
        Like(self, format!("{}%", pattern.into())).into_column()
    }

    /// Check if the column ends with a string pattern.
    pub fn ends_with(&self, pattern: impl Into<String>) -> Column<'t, S, bool> {
        Like(self, format!("%{}", pattern.into())).into_column()
    }

    /// Check if the column contains a string pattern.
    pub fn contains(&self, pattern: impl Into<String>) -> Column<'t, S, bool> {
        Like(self, format!("%{}%", pattern.into())).into_column()
    }

    /// Check if the column matches to a pattern [pattern ref](https://www.w3schools.com/sql/sql_like.asp).
    pub fn like(&self, pattern: impl Into<String> + Clone + 't) -> Column<'t, S, bool> {
        Like(self, pattern).into_column()
    }

    /// Check if the column not matches a pattern [pattern ref](https://www.w3schools.com/sql/sql_like.asp).
    pub fn not_like(&self, pattern: impl Into<String> + Clone + 't) -> Column<'t, S, bool> {
        NotLike(self, pattern).into_column()
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

impl<'t, S, T: IntoColumn<'t, S, Typ = X>, X: MyTyp<Sql: Nullable>> IntoColumn<'t, S>
    for Option<T>
{
    type Owned = Option<T::Owned>;
    fn into_owned(self) -> Self::Owned {
        self.map(IntoColumn::into_owned)
    }
}

impl Typed for &str {
    type Typ = String;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

impl<'t, S> IntoColumn<'t, S> for &str {
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

impl<'t, S> IntoColumn<'t, S> for String {
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

impl<'t, S> IntoColumn<'t, S> for bool {
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

impl<'t, S> IntoColumn<'t, S> for i64 {
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

impl<'t, S> IntoColumn<'t, S> for f64 {
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

impl<'t, S, T> IntoColumn<'t, S> for &T
where
    T: IntoColumn<'t, S>,
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

impl<'t, S> IntoColumn<'t, S> for UnixEpoch {
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
    #[doc(hidden)]
    type Sql;
}

impl<T: Table> MyTyp for T {
    const TYP: hash::ColumnType = hash::ColumnType::Integer;
    const FK: Option<(&'static str, &'static str)> = Some((T::NAME, T::ID));
    type Out<'t> = TableRow<'t, Self>;
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
    type Out<'t> = NoTable;
    type Sql = i64;
}

impl FromSql for NoTable {
    fn column_result(_: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        unreachable!()
    }
}

/// Values of this type reference a collumn in a query.
///
/// - The lifetime parameter `'t` specifies in which query the collumn exists.
/// - The type parameter `S` specifies the expected schema of the query.
/// - And finally the type paramter `T` specifies the type of the column.
///
/// [Column] implements [Deref] to have table extension methods in case the type is a table type.
pub struct Column<'t, S, T>(
    pub(crate) Rc<dyn Typed<Typ = T> + 't>,
    pub(crate) PhantomData<fn(&'t S) -> &'t S>,
);

impl<'t, S, T> Clone for Column<'t, S, T> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), PhantomData)
    }
}

impl<'t, S, T> Typed for Column<'t, S, T> {
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

impl<'t, S: 't, T: 't> IntoColumn<'t, S> for Column<'t, S, T> {
    type Owned = Self;
    fn into_owned(self) -> Self::Owned {
        self
    }
}

impl<'t, S, T: Table> Deref for Column<'t, S, T> {
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
