pub mod operations;

use std::{marker::PhantomData, ops::Deref, rc::Rc};

use operations::{
    Add, And, AndThen, AsFloat, Assume, Eq, Glob, IsNotNull, Like, Lt, Not, Or, UnwrapOr,
};
use ref_cast::RefCast;
use sea_query::{Alias, Expr, Nullable, SelectStatement, SimpleExpr};

use crate::{
    alias::{Field, MyAlias, RawAlias},
    ast::{MySelect, Source},
    db::{TableRow, TableRowInner},
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

#[diagnostic::on_unimplemented(
    message = "Columns with type `{Self}` can not be checked for equality",
    note = "`EqTyp` is also implemented for all table types"
)]
pub trait EqTyp {}

impl EqTyp for String {}
impl EqTyp for i64 {}
impl EqTyp for f64 {}
impl EqTyp for bool {}
#[diagnostic::do_not_recommend]
impl<T: Table> EqTyp for T {}

/// Typ does not depend on scope, so it gets its own trait
pub trait Typed {
    /// TODO: somehow make this documentation visible?
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
    type Owned: Typed<Typ = Self::Typ> + 'static;

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

impl<'t, S, T: EqTyp + 'static> Column<'t, S, T> {
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

impl<'t, S, Typ: 'static> Column<'t, S, Option<Typ>> {
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

    pub fn map<O: MyTyp<Sql: Nullable>>(
        &self,
        f: impl for<'x> FnOnce(Mappable<'t, 'x, S, Typ>) -> Column<'x, S, O>,
    ) -> Column<'t, S, Option<O>> {
        self.and_then(|x| Some(f(x)).into_column())
    }

    pub fn and_then<O: 'static>(
        &self,
        f: impl for<'x> FnOnce(Mappable<'t, 'x, S, Typ>) -> Column<'x, S, Option<O>>,
    ) -> Column<'t, S, Option<O>> {
        let mappable = Mappable {
            _p: PhantomData,
            actual: Column(Assume(self).into_column().0, PhantomData),
        };
        AndThen(self, f(mappable)).into_column()
    }
}

/// This struct adds the implied bound 't: 'x.
/// Sadly this only adds the ability to use [TableRow] inside of the mapping
/// and it does not allow using multiple [Column].
pub struct Mappable<'t, 'x, S, Typ> {
    _p: PhantomData<&'x &'t ()>,
    actual: Column<'x, S, Typ>,
}

impl<'x, S, Typ> Deref for Mappable<'_, 'x, S, Typ> {
    type Target = Column<'x, S, Typ>;

    fn deref(&self) -> &Self::Target {
        &self.actual
    }
}

impl<'t, S> Column<'t, S, i64> {
    /// Convert the [i64] column to [f64] type.
    pub fn as_float(&self) -> Column<'t, S, f64> {
        AsFloat(self).into_column()
    }
}

impl<'t, S> Column<'t, S, String> {
    /// Check if the column starts with the string pattern.
    ///
    /// Matches case-sensitive. The pattern gets automatically escaped.
    pub fn starts_with(&self, pattern: impl AsRef<str>) -> Column<'t, S, bool> {
        Glob(self, format!("{}*", escape_glob(pattern))).into_column()
    }

    /// Check if the column ends with the string pattern.
    ///
    /// Matches case-sensitive. The pattern gets automatically escaped.
    pub fn ends_with(&self, pattern: impl AsRef<str>) -> Column<'t, S, bool> {
        Glob(self, format!("*{}", escape_glob(pattern))).into_column()
    }

    /// Check if the column contains the string pattern.
    ///
    /// Matches case-sensitive. The pattern gets automatically escaped.
    pub fn contains(&self, pattern: impl AsRef<str>) -> Column<'t, S, bool> {
        Glob(self, format!("*{}*", escape_glob(pattern))).into_column()
    }

    /// Check if the column matches the pattern [docs](https://www.sqlite.org/lang_expr.html#like).
    ///
    /// As noted in the docs, it is **case-insensitive** for ASCII characters. Other characters are case-sensitive.
    /// For creating patterns it uses `%` as a wildcard for any sequence of characters and `_` for any single character.
    /// Special characters should be escaped with `\`.
    pub fn like(&self, pattern: impl Into<String> + Clone + 't) -> Column<'t, S, bool> {
        Like(self, pattern.into()).into_column()
    }

    /// Check if the column matches the pattern [docs](https://www.sqlite.org/lang_expr.html#like).
    ///
    /// This is a case-sensitive version of [like](Self::like). It uses Unix file globbing syntax for wild
    /// cards. `*` matches any sequence of characters and `?` matches any single character. `[0-9]` matches
    /// any single digit and `[a-z]` matches any single lowercase letter. `^` negates the pattern.
    pub fn glob(&self, rhs: impl IntoColumn<'t, S, Typ = String>) -> Column<'t, S, bool> {
        Glob(self, rhs).into_column()
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
    type Out<'t>;
    #[doc(hidden)]
    type Sql;
    #[doc(hidden)]
    fn from_sql<'a>(
        value: rusqlite::types::ValueRef<'_>,
    ) -> rusqlite::types::FromSqlResult<Self::Out<'a>>;
}

impl<T: Table> MyTyp for T {
    const TYP: hash::ColumnType = hash::ColumnType::Integer;
    const FK: Option<(&'static str, &'static str)> = Some((T::NAME, T::ID));
    type Out<'t> = TableRow<'t, Self>;
    type Sql = i64;

    fn from_sql<'a>(
        value: rusqlite::types::ValueRef<'_>,
    ) -> rusqlite::types::FromSqlResult<Self::Out<'a>> {
        Ok(TableRow {
            _p: PhantomData,
            _local: PhantomData,
            inner: TableRowInner {
                _p: PhantomData,
                idx: value.as_i64()?,
            },
        })
    }
}

impl MyTyp for i64 {
    const TYP: hash::ColumnType = hash::ColumnType::Integer;
    type Out<'t> = Self;
    type Sql = i64;

    fn from_sql<'a>(
        value: rusqlite::types::ValueRef<'_>,
    ) -> rusqlite::types::FromSqlResult<Self::Out<'a>> {
        value.as_i64()
    }
}

impl MyTyp for f64 {
    const TYP: hash::ColumnType = hash::ColumnType::Float;
    type Out<'t> = Self;
    type Sql = f64;

    fn from_sql<'a>(
        value: rusqlite::types::ValueRef<'_>,
    ) -> rusqlite::types::FromSqlResult<Self::Out<'a>> {
        value.as_f64()
    }
}

impl MyTyp for bool {
    const TYP: hash::ColumnType = hash::ColumnType::Integer;
    type Out<'t> = Self;
    type Sql = bool;

    fn from_sql<'a>(
        value: rusqlite::types::ValueRef<'_>,
    ) -> rusqlite::types::FromSqlResult<Self::Out<'a>> {
        Ok(value.as_i64()? != 0)
    }
}

impl MyTyp for String {
    const TYP: hash::ColumnType = hash::ColumnType::String;
    type Out<'t> = Self;
    type Sql = String;

    fn from_sql<'a>(
        value: rusqlite::types::ValueRef<'_>,
    ) -> rusqlite::types::FromSqlResult<Self::Out<'a>> {
        Ok(value.as_str()?.to_owned())
    }
}

impl<T: MyTyp> MyTyp for Option<T> {
    const TYP: hash::ColumnType = T::TYP;
    const NULLABLE: bool = true;
    const FK: Option<(&'static str, &'static str)> = T::FK;
    type Out<'t> = Option<T::Out<'t>>;
    type Sql = T::Sql;

    fn from_sql<'a>(
        value: rusqlite::types::ValueRef<'_>,
    ) -> rusqlite::types::FromSqlResult<Self::Out<'a>> {
        if value.data_type() == rusqlite::types::Type::Null {
            Ok(None)
        } else {
            Ok(Some(T::from_sql(value)?))
        }
    }
}

impl MyTyp for NoTable {
    const TYP: hash::ColumnType = hash::ColumnType::Integer;
    type Out<'t> = NoTable;
    type Sql = i64;

    fn from_sql<'a>(
        _value: rusqlite::types::ValueRef<'_>,
    ) -> rusqlite::types::FromSqlResult<Self::Out<'a>> {
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
    pub(crate) Rc<dyn Typed<Typ = T>>,
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

pub struct DynTyped<Typ>(Rc<dyn Typed<Typ = Typ>>);

impl<Typ> Typed for DynTyped<Typ> {
    type Typ = Typ;

    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b)
    }

    fn build_table(&self, b: crate::value::ValueBuilder) -> MyAlias
    where
        Self::Typ: Table,
    {
        self.0.build_table(b)
    }
}

impl<'t, S: 't, T: 'static> IntoColumn<'t, S> for Column<'t, S, T> {
    type Owned = DynTyped<T>;
    fn into_owned(self) -> Self::Owned {
        DynTyped(self.0)
    }
}

impl<'t, S, T: Table> Deref for Column<'t, S, T> {
    type Target = T::Ext<Self>;

    fn deref(&self) -> &Self::Target {
        RefCast::ref_cast(self)
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
