pub mod optional;
pub mod trivial;

use std::{marker::PhantomData, ops::Deref, rc::Rc};

use ref_cast::RefCast;
use sea_query::{
    Alias, ExprTrait, Nullable, SelectStatement, SimpleExpr, extension::sqlite::SqliteExpr,
};

use crate::{
    IntoSelect, Select, Table,
    alias::{Field, MyAlias, RawAlias},
    ast::{MySelect, Source},
    db::{TableRow, TableRowInner},
    hash,
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

    pub fn get_unique<T: Table>(self, conds: Vec<(&'static str, SimpleExpr)>) -> SimpleExpr {
        let source = Source {
            kind: crate::ast::SourceKind::Implicit(T::NAME.to_owned()),
            conds: conds.into_iter().map(|x| (Field::Str(x.0), x.1)).collect(),
        };

        let new_alias = || self.inner.scope.new_alias();
        let table = self.inner.extra.get_or_init(source, new_alias);
        sea_query::Expr::col((*table, Alias::new(T::ID))).into()
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
impl EqTyp for Vec<u8> {}
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
    fn build_table(&self, b: ValueBuilder) -> MyAlias
    where
        Self::Typ: Table,
    {
        b.get_join::<Self::Typ>(self.build_expr(b))
    }
}

pub(crate) trait Private {}

/// Trait for all values that can be used as expressions in queries.
///
/// You can not (yet) implement this trait yourself!
pub trait IntoExpr<'column, S>: Private + Clone {
    /// The type of the expression.
    type Typ: MyTyp;

    /// Turn this value into an [Expr].
    fn into_expr(self) -> Expr<'column, S, Self::Typ>;
}

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
impl<'column, S, T: IntoExpr<'column, S, Typ = X>, X: MyTyp<Sql: Nullable>> IntoExpr<'column, S>
    for Option<T>
{
    type Typ = Option<X>;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::new(self.map(|x| x.into_expr().inner))
    }
}

impl Typed for String {
    type Typ = String;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(self)
    }
}

impl Private for String {}
impl<'column, S> IntoExpr<'column, S> for String {
    type Typ = String;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::new(self)
    }
}

impl Private for &str {}
impl<'column, S> IntoExpr<'column, S> for &str {
    type Typ = String;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::new(self.to_owned())
    }
}

impl Typed for Vec<u8> {
    type Typ = Vec<u8>;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(self.to_owned())
    }
}

impl Private for Vec<u8> {}
impl<'column, S> IntoExpr<'column, S> for Vec<u8> {
    type Typ = Vec<u8>;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::new(self)
    }
}

impl Private for &[u8] {}
impl<'column, S> IntoExpr<'column, S> for &[u8] {
    type Typ = Vec<u8>;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::new(self.to_owned())
    }
}

impl Typed for bool {
    type Typ = bool;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

impl Private for bool {}
impl<'column, S> IntoExpr<'column, S> for bool {
    type Typ = bool;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::new(self)
    }
}

impl Typed for i64 {
    type Typ = i64;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

impl Private for i64 {}
impl<'column, S> IntoExpr<'column, S> for i64 {
    type Typ = i64;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::new(self)
    }
}

impl Typed for f64 {
    type Typ = f64;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        SimpleExpr::from(*self)
    }
}

impl Private for f64 {}
impl<'column, S> IntoExpr<'column, S> for f64 {
    type Typ = f64;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::new(self)
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
impl<'column, S, T> IntoExpr<'column, S> for &T
where
    T: IntoExpr<'column, S>,
{
    type Typ = T::Typ;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        T::into_expr(self.clone())
    }
}

/// Use this a value in a query to get the current datetime as a number.
#[derive(Clone, Copy)]
pub struct UnixEpoch;

impl Typed for UnixEpoch {
    type Typ = i64;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        sea_query::Expr::col(RawAlias("unixepoch('now')".to_owned())).into()
    }
}

impl Private for UnixEpoch {}
impl<'column, S> IntoExpr<'column, S> for UnixEpoch {
    type Typ = i64;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::new(self)
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

#[diagnostic::do_not_recommend]
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
assert_impl_all!(String: Nullable);

impl SecretFromSql<'_> for String {
    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(value.as_str()?.to_owned())
    }
}

impl MyTyp for Vec<u8> {
    const TYP: hash::ColumnType = hash::ColumnType::Blob;
    type Out<'t> = Self;
    type Sql = Vec<u8>;
}
assert_impl_all!(Vec<u8>: Nullable);

impl SecretFromSql<'_> for Vec<u8> {
    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(value.as_blob()?.to_owned())
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

/// This is an expression that can be used in queries.
///
/// - The lifetime parameter `'column` specifies which columns need to be in scope.
/// - The type parameter `S` specifies the expected schema of the query.
/// - And finally the type paramter `T` specifies the type of the expression.
///
/// [Expr] implements [Deref] to have table extension methods in case the type is a table type.
pub struct Expr<'column, S, T> {
    pub(crate) inner: DynTyped<T>,
    pub(crate) _p: PhantomData<&'column ()>,
    pub(crate) _p2: PhantomData<S>,
}

impl<'column, S, T: 'static> Expr<'column, S, T> {
    /// Extremely easy to use API. Should only be used by the macro to implement migrations.
    #[doc(hidden)]
    pub fn _migrate<OldS>(prev: impl IntoExpr<'column, OldS>) -> Self {
        Self::new(MigratedExpr {
            prev: prev.into_expr().inner.erase(),
            _p: PhantomData,
        })
    }
}

pub fn new_column<'x, S, T: 'static>(val: impl Typed<Typ = T> + 'static) -> Expr<'x, S, T> {
    Expr::new(val)
}

pub fn new_dummy<'x, S, T: MyTyp>(
    val: impl Typed<Typ = T> + 'static,
) -> Select<'x, 'x, S, T::Out<'x>> {
    IntoSelect::into_select(Expr::new(val))
}

pub fn into_owned<'x, S, T>(val: impl IntoExpr<'x, S, Typ = T>) -> DynTyped<T> {
    val.into_expr().inner
}

struct AdHoc<F, T>(F, PhantomData<T>);
impl<F: Fn(ValueBuilder) -> SimpleExpr, T> Typed for AdHoc<F, T> {
    type Typ = T;

    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        (self.0)(b)
    }
}

impl<S, T: 'static> Expr<'_, S, T> {
    pub(crate) fn adhoc(f: impl 'static + Fn(ValueBuilder) -> SimpleExpr) -> Self {
        Self::new(AdHoc(f, PhantomData))
    }

    pub(crate) fn new(val: impl Typed<Typ = T> + 'static) -> Self {
        Self {
            inner: DynTyped(Rc::new(val)),
            _p: PhantomData,
            _p2: PhantomData,
        }
    }
}

impl<'column, S, T> Clone for Expr<'column, S, T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            _p: self._p.clone(),
            _p2: self._p2.clone(),
        }
    }
}

// TODO: remove this and replace with `Private`
impl<'column, S, T: 'static> Typed for Expr<'column, S, T> {
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

pub struct MigratedExpr<Typ> {
    prev: DynTypedExpr,
    _p: PhantomData<Typ>,
}

impl<Typ> Typed for MigratedExpr<Typ> {
    type Typ = Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.prev.0(b)
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

impl<'column, S, T> Private for Expr<'column, S, T> {}
impl<'column, S, T: MyTyp> IntoExpr<'column, S> for Expr<'column, S, T> {
    type Typ = T;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        self
    }
}

impl<'column, S, T: Table> Deref for Expr<'column, S, T> {
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
