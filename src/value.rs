pub mod operations;
pub mod optional;
pub mod trivial;

use std::{marker::PhantomData, ops::Deref, rc::Rc};

use operations::{Add, And, AsFloat, Eq, Glob, IsNotNull, Like, Lt, Not, Or, UnwrapOr};
use ref_cast::RefCast;
use sea_query::{Alias, Expr, Nullable, SelectStatement, SimpleExpr};
use trivial::FromColumn;

use crate::{
    alias::{Field, MyAlias, RawAlias},
    ast::{MySelect, Source},
    db::{TableRow, TableRowInner},
    hash,
    migrate::NoTable,
    Dummy, Table,
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

pub(crate) trait Private {}

/// Trait for all values that can be used as columns in queries.
///
/// You can not (yet) implement this trait yourself!
pub trait IntoColumn<'column, S>: Private + Clone {
    /// The type of the column.
    type Typ: MyTyp;

    /// Turn this value into a [Column].
    fn into_column(self) -> Column<'column, S, Self::Typ>;

    /// Convert the column to a dummy using the [FromColumn] implementation.
    fn into_trivial<'transaction, X: FromColumn<'transaction, S, Self::Typ>>(
        self,
    ) -> Dummy<'column, 'transaction, S, X> {
        X::from_column(self.into_column())
    }
}

impl<'column, S, T: NumTyp> Column<'column, S, T> {
    /// Add two columns together.
    pub fn add(&self, rhs: impl IntoColumn<'column, S, Typ = T>) -> Column<'column, S, T> {
        Column::new(Add(self.inner.clone(), rhs.into_column().inner))
    }

    /// Compute the less than operator of two columns.
    pub fn lt(&self, rhs: impl IntoColumn<'column, S, Typ = T>) -> Column<'column, S, bool> {
        Column::new(Lt(self.inner.clone(), rhs.into_column().inner))
    }
}

impl<'column, S, T: EqTyp + 'static> Column<'column, S, T> {
    /// Check whether two columns are equal.
    pub fn eq(&self, rhs: impl IntoColumn<'column, S, Typ = T>) -> Column<'column, S, bool> {
        Column::new(Eq(self.inner.clone(), rhs.into_column().inner))
    }
}

impl<'column, S> Column<'column, S, bool> {
    /// Checks whether a column is false.
    pub fn not(&self) -> Column<'column, S, bool> {
        Column::new(Not(self.inner.clone()))
    }

    /// Check if two columns are both true.
    pub fn and(&self, rhs: impl IntoColumn<'column, S, Typ = bool>) -> Column<'column, S, bool> {
        Column::new(And(self.inner.clone(), rhs.into_column().inner))
    }

    /// Check if one of two columns is true.
    pub fn or(&self, rhs: impl IntoColumn<'column, S, Typ = bool>) -> Column<'column, S, bool> {
        Column::new(Or(self.inner.clone(), rhs.into_column().inner))
    }
}

impl<'column, S, Typ: 'static> Column<'column, S, Option<Typ>> {
    /// Use the first column if it is [Some], otherwise use the second column.
    pub fn unwrap_or(&self, rhs: impl IntoColumn<'column, S, Typ = Typ>) -> Column<'column, S, Typ>
    where
        Self: IntoColumn<'column, S, Typ = Option<Typ>>,
    {
        Column::new(UnwrapOr(self.inner.clone(), rhs.into_column().inner))
    }

    /// Check that the column is [Some].
    pub fn is_some(&self) -> Column<'column, S, bool> {
        Column::new(IsNotNull(self.inner.clone()))
    }
}

impl<'column, S> Column<'column, S, i64> {
    /// Convert the [i64] column to [f64] type.
    pub fn as_float(&self) -> Column<'column, S, f64> {
        Column::new(AsFloat(self.inner.clone()))
    }
}

impl<'column, S> Column<'column, S, String> {
    /// Check if the column starts with the string pattern.
    ///
    /// Matches case-sensitive. The pattern gets automatically escaped.
    pub fn starts_with(&self, pattern: impl AsRef<str>) -> Column<'column, S, bool> {
        Column::new(Glob(
            self.inner.clone(),
            format!("{}*", escape_glob(pattern)),
        ))
    }

    /// Check if the column ends with the string pattern.
    ///
    /// Matches case-sensitive. The pattern gets automatically escaped.
    pub fn ends_with(&self, pattern: impl AsRef<str>) -> Column<'column, S, bool> {
        Column::new(Glob(
            self.inner.clone(),
            format!("*{}", escape_glob(pattern)),
        ))
    }

    /// Check if the column contains the string pattern.
    ///
    /// Matches case-sensitive. The pattern gets automatically escaped.
    pub fn contains(&self, pattern: impl AsRef<str>) -> Column<'column, S, bool> {
        Column::new(Glob(
            self.inner.clone(),
            format!("*{}*", escape_glob(pattern)),
        ))
    }

    /// Check if the column matches the pattern [docs](https://www.sqlite.org/lang_expr.html#like).
    ///
    /// As noted in the docs, it is **case-insensitive** for ASCII characters. Other characters are case-sensitive.
    /// For creating patterns it uses `%` as a wildcard for any sequence of characters and `_` for any single character.
    /// Special characters should be escaped with `\`.
    pub fn like(&self, pattern: impl Into<String>) -> Column<'column, S, bool> {
        Column::new(Like(self.inner.clone(), pattern.into()))
    }

    /// Check if the column matches the pattern [docs](https://www.sqlite.org/lang_expr.html#like).
    ///
    /// This is a case-sensitive version of [like](Self::like). It uses Unix file globbing syntax for wild
    /// cards. `*` matches any sequence of characters and `?` matches any single character. `[0-9]` matches
    /// any single digit and `[a-z]` matches any single lowercase letter. `^` negates the pattern.
    pub fn glob(&self, rhs: impl IntoColumn<'column, S, Typ = String>) -> Column<'column, S, bool> {
        Column::new(Glob(self.inner.clone(), rhs.into_column().inner))
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

impl<T> Private for Option<T> {}
impl<'column, S, T: IntoColumn<'column, S, Typ = X>, X: MyTyp<Sql: Nullable>> IntoColumn<'column, S>
    for Option<T>
{
    type Typ = Option<X>;
    fn into_column(self) -> Column<'column, S, Self::Typ> {
        Column::new(self.map(|x| x.into_column().inner))
    }
}

impl Typed for &str {
    type Typ = String;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

impl Private for &str {}
impl<'column, S> IntoColumn<'column, S> for &str {
    type Typ = String;
    fn into_column(self) -> Column<'column, S, Self::Typ> {
        Column::new(self.to_owned())
    }
}

impl Typed for String {
    type Typ = String;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(self)
    }
}

impl Private for String {}
impl<'column, S> IntoColumn<'column, S> for String {
    type Typ = String;
    fn into_column(self) -> Column<'column, S, Self::Typ> {
        Column::new(self)
    }
}

impl Typed for bool {
    type Typ = bool;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

impl Private for bool {}
impl<'column, S> IntoColumn<'column, S> for bool {
    type Typ = bool;
    fn into_column(self) -> Column<'column, S, Self::Typ> {
        Column::new(self)
    }
}

impl Typed for i64 {
    type Typ = i64;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

impl Private for i64 {}
impl<'column, S> IntoColumn<'column, S> for i64 {
    type Typ = i64;
    fn into_column(self) -> Column<'column, S, Self::Typ> {
        Column::new(self)
    }
}

impl Typed for f64 {
    type Typ = f64;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

impl Private for f64 {}
impl<'column, S> IntoColumn<'column, S> for f64 {
    type Typ = f64;
    fn into_column(self) -> Column<'column, S, Self::Typ> {
        Column::new(self)
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

impl<T> Private for &T {}
impl<'column, S, T> IntoColumn<'column, S> for &T
where
    T: IntoColumn<'column, S>,
{
    type Typ = T::Typ;
    fn into_column(self) -> Column<'column, S, Self::Typ> {
        T::into_column(self.clone())
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

impl Private for UnixEpoch {}
impl<'column, S> IntoColumn<'column, S> for UnixEpoch {
    type Typ = i64;
    fn into_column(self) -> Column<'column, S, Self::Typ> {
        Column::new(self)
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
    type Out<'t>: SecretFromSql<'t>;
    #[doc(hidden)]
    type Sql;
}

pub(crate) trait SecretFromSql<'t>: Sized {
    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self>;
}

impl<T: Table> MyTyp for T {
    const TYP: hash::ColumnType = hash::ColumnType::Integer;
    const FK: Option<(&'static str, &'static str)> = Some((T::NAME, T::ID));
    type Out<'t> = TableRow<'t, Self>;
    type Sql = i64;
}

impl<'t, T> SecretFromSql<'t> for TableRow<'t, T> {
    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
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
}

impl SecretFromSql<'_> for i64 {
    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        value.as_i64()
    }
}

impl MyTyp for f64 {
    const TYP: hash::ColumnType = hash::ColumnType::Float;
    type Out<'t> = Self;
    type Sql = f64;
}

impl SecretFromSql<'_> for f64 {
    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        value.as_f64()
    }
}

impl MyTyp for bool {
    const TYP: hash::ColumnType = hash::ColumnType::Integer;
    type Out<'t> = Self;
    type Sql = bool;
}

impl SecretFromSql<'_> for bool {
    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(value.as_i64()? != 0)
    }
}

impl MyTyp for String {
    const TYP: hash::ColumnType = hash::ColumnType::String;
    type Out<'t> = Self;
    type Sql = String;
}

impl SecretFromSql<'_> for String {
    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(value.as_str()?.to_owned())
    }
}

impl<T: MyTyp> MyTyp for Option<T> {
    const TYP: hash::ColumnType = T::TYP;
    const NULLABLE: bool = true;
    const FK: Option<(&'static str, &'static str)> = T::FK;
    type Out<'t> = Option<T::Out<'t>>;
    type Sql = T::Sql;
}

impl<'t, T: SecretFromSql<'t>> SecretFromSql<'t> for Option<T> {
    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
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
}

impl SecretFromSql<'_> for NoTable {
    fn from_sql(_value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        unreachable!()
    }
}

/// Values of this type reference a column in a query.
///
/// - The lifetime parameter `'column` specifies in which query the column exists.
/// - The type parameter `S` specifies the expected schema of the query.
/// - And finally the type paramter `T` specifies the type of the column.
///
/// [Column] implements [Deref] to have table extension methods in case the type is a table type.
pub struct Column<'column, S, T> {
    pub(crate) inner: DynTyped<T>,
    pub(crate) _p: PhantomData<&'column ()>,
    pub(crate) _p2: PhantomData<S>,
}

pub fn new_column<'x, S, T>(val: impl Typed<Typ = T> + 'static) -> Column<'x, S, T> {
    Column::new(val)
}

pub fn into_owned<'x, S, T>(val: impl IntoColumn<'x, S, Typ = T>) -> DynTyped<T> {
    val.into_column().inner
}

impl<S, T> Column<'_, S, T> {
    pub(crate) fn new(val: impl Typed<Typ = T> + 'static) -> Self {
        Self {
            inner: DynTyped(Rc::new(val)),
            _p: PhantomData,
            _p2: PhantomData,
        }
    }
}

impl<'column, S, T> Clone for Column<'column, S, T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            _p: self._p.clone(),
            _p2: self._p2.clone(),
        }
    }
}

// TODO: remove this and replace with `Private`
impl<'column, S, T: 'static> Typed for Column<'column, S, T> {
    type Typ = T;

    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.inner.0.as_ref().build_expr(b)
    }

    fn build_table(&self, b: crate::value::ValueBuilder) -> MyAlias
    where
        Self::Typ: Table,
    {
        self.inner.0.as_ref().build_table(b)
    }
}

pub struct DynTypedExpr(pub(crate) Box<dyn Fn(ValueBuilder) -> SimpleExpr>);

impl<Typ: 'static> DynTyped<Typ> {
    pub fn erase(self) -> DynTypedExpr {
        DynTypedExpr(Box::new(move |b| self.build_expr(b)))
    }
}

pub struct DynTyped<Typ>(pub(crate) Rc<dyn Typed<Typ = Typ>>);

impl<T> Clone for DynTyped<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<Typ: 'static> Typed for DynTyped<Typ> {
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

impl<'column, S, T> Private for Column<'column, S, T> {}
impl<'column, S, T: MyTyp> IntoColumn<'column, S> for Column<'column, S, T> {
    type Typ = T;
    fn into_column(self) -> Column<'column, S, Self::Typ> {
        self
    }
}

impl<'column, S, T: Table> Deref for Column<'column, S, T> {
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
