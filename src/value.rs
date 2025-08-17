pub mod aggregate;
mod operations;
pub mod optional;
pub mod trivial;

use std::{cell::OnceCell, fmt::Debug, marker::PhantomData, ops::Deref, rc::Rc};

use sea_query::{Alias, Nullable, SelectStatement};

use crate::{
    IntoSelect, Select, Table,
    alias::{Field, MyAlias, Scope},
    ast::{MySelect, Source},
    db::{Join, TableRow, TableRowInner},
    hash,
    mymap::MyMap,
};

#[derive(Default)]
pub struct ValueBuilder {
    pub(crate) from: Rc<MySelect>,
    // only used for tables
    pub(super) scope: Scope,
    // implicit joins
    pub(super) extra: MyMap<Source, MyAlias>,
    // calculating these results
    pub(super) forwarded: MyMap<MyTableRef, (&'static str, DynTypedExpr, MyAlias)>,
}

impl ValueBuilder {
    pub(crate) fn get_aggr(
        &mut self,
        aggr: Rc<SelectStatement>,
        conds: Vec<sea_query::Expr>,
    ) -> MyAlias {
        let source = Source {
            kind: crate::ast::SourceKind::Aggregate(aggr),
            conds: conds
                .into_iter()
                .enumerate()
                .map(|(idx, expr)| (Field::U64(MyAlias::new(idx)), expr))
                .collect(),
        };
        let new_alias = || self.scope.new_alias();
        *self.extra.get_or_init(source, new_alias)
    }

    pub(crate) fn get_join<T: Table>(&mut self, expr: sea_query::Expr) -> MyAlias {
        let source = Source {
            kind: crate::ast::SourceKind::Implicit(T::NAME.to_owned()),
            conds: vec![(Field::Str(T::ID), expr)],
        };
        let new_alias = || self.scope.new_alias();
        *self.extra.get_or_init(source, new_alias)
    }

    pub fn get_unique<T: Table>(
        &mut self,
        conds: Box<[(&'static str, sea_query::Expr)]>,
    ) -> sea_query::Expr {
        let source = Source {
            kind: crate::ast::SourceKind::Implicit(T::NAME.to_owned()),
            conds: conds.into_iter().map(|x| (Field::Str(x.0), x.1)).collect(),
        };

        let new_alias = || self.scope.new_alias();
        let table = self.extra.get_or_init(source, new_alias);
        sea_query::Expr::col((*table, Alias::new(T::ID))).into()
    }

    pub fn get_table<T: Table>(&mut self, table: MyTableRef) -> MyAlias {
        if Rc::ptr_eq(&self.from.scope_rc, &table.scope_rc) {
            MyAlias::new(table.idx)
        } else {
            self.forwarded
                .get_or_init(table.clone(), || {
                    (
                        T::NAME,
                        DynTyped::new(Join::<T>::new(table)).erase(),
                        self.scope.new_alias(),
                    )
                })
                .2
        }
    }
}

#[derive(Clone)]
pub struct MyTableRef {
    pub(crate) scope_rc: Rc<()>,
    pub(crate) idx: usize,
}

impl PartialEq for MyTableRef {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.scope_rc, &other.scope_rc) && self.idx == other.idx
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
pub trait EqTyp: MyTyp {}

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
    fn build_expr(&self, b: &mut ValueBuilder) -> sea_query::Expr;
    #[doc(hidden)]
    fn build_table(&self, b: &mut ValueBuilder) -> MyAlias
    where
        Self::Typ: Table,
    {
        let expr = self.build_expr(b);
        b.get_join::<Self::Typ>(expr)
    }
}

/// Trait for all values that can be used as expressions in queries.
pub trait IntoExpr<'column, S> {
    /// The type of the expression.
    type Typ: MyTyp;

    /// Turn this value into an [Expr].
    fn into_expr(self) -> Expr<'column, S, Self::Typ>;
}

impl<T: Typed<Typ = X>, X: MyTyp<Sql: Nullable>> Typed for Option<T> {
    type Typ = Option<T::Typ>;

    fn build_expr(&self, b: &mut ValueBuilder) -> sea_query::Expr {
        self.as_ref()
            .map(|x| T::build_expr(x, b))
            .unwrap_or(X::Sql::null().into())
    }
}

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
    fn build_expr(&self, _: &mut ValueBuilder) -> sea_query::Expr {
        sea_query::Expr::from(self)
    }
}

impl<'column, S> IntoExpr<'column, S> for String {
    type Typ = String;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::new(self)
    }
}

impl<'column, S> IntoExpr<'column, S> for &str {
    type Typ = String;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::new(self.to_owned())
    }
}

impl Typed for Vec<u8> {
    type Typ = Vec<u8>;
    fn build_expr(&self, _: &mut ValueBuilder) -> sea_query::Expr {
        sea_query::Expr::from(self.to_owned())
    }
}

impl<'column, S> IntoExpr<'column, S> for Vec<u8> {
    type Typ = Vec<u8>;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::new(self)
    }
}

impl<'column, S> IntoExpr<'column, S> for &[u8] {
    type Typ = Vec<u8>;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::new(self.to_owned())
    }
}

impl Typed for bool {
    type Typ = bool;
    fn build_expr(&self, _: &mut ValueBuilder) -> sea_query::Expr {
        sea_query::Expr::from(*self)
    }
}

impl<'column, S> IntoExpr<'column, S> for bool {
    type Typ = bool;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::new(self)
    }
}

impl Typed for i64 {
    type Typ = i64;
    fn build_expr(&self, _: &mut ValueBuilder) -> sea_query::Expr {
        sea_query::Expr::from(*self)
    }
}

impl<'column, S> IntoExpr<'column, S> for i64 {
    type Typ = i64;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::new(self)
    }
}

impl Typed for f64 {
    type Typ = f64;
    fn build_expr(&self, _: &mut ValueBuilder) -> sea_query::Expr {
        sea_query::Expr::from(*self)
    }
}

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
    fn build_expr(&self, b: &mut ValueBuilder) -> sea_query::Expr {
        T::build_expr(self, b)
    }
    fn build_table(&self, b: &mut ValueBuilder) -> MyAlias
    where
        Self::Typ: Table,
    {
        T::build_table(self, b)
    }
}

impl<'column, S, T> IntoExpr<'column, S> for &T
where
    T: IntoExpr<'column, S> + Clone,
{
    type Typ = T::Typ;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        T::into_expr(self.clone())
    }
}

/// Use this a value in a query to get the current datetime as a number of miliseconds.
#[derive(Clone, Copy)]
pub struct UnixEpoch;

impl Typed for UnixEpoch {
    type Typ = i64;
    fn build_expr(&self, _: &mut ValueBuilder) -> sea_query::Expr {
        sea_query::Expr::cust("unixepoch('now')").into()
    }
}

impl<'column, S> IntoExpr<'column, S> for UnixEpoch {
    type Typ = i64;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::new(self)
    }
}

pub trait MyTyp: 'static {
    type Prev: MyTyp;
    const NULLABLE: bool = false;
    const TYP: hash::ColumnType;
    const FK: Option<(&'static str, &'static str)> = None;
    type Out: SecretFromSql;
    type Ext<'t>;
    type Sql;
}

pub(crate) trait SecretFromSql: Sized {
    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self>;
}

#[diagnostic::do_not_recommend]
impl<T: Table> MyTyp for T {
    type Prev = T::MigrateFrom;
    const TYP: hash::ColumnType = hash::ColumnType::Integer;
    const FK: Option<(&'static str, &'static str)> = Some((T::NAME, T::ID));
    type Out = TableRow<Self>;
    type Ext<'t> = T::Ext2<'t>;
    type Sql = i64;
}

impl<T: Table> SecretFromSql for TableRow<T> {
    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(TableRow {
            _local: PhantomData,
            inner: TableRowInner {
                _p: PhantomData,
                idx: value.as_i64()?,
            },
        })
    }
}

impl MyTyp for i64 {
    type Prev = Self;
    const TYP: hash::ColumnType = hash::ColumnType::Integer;
    type Out = Self;
    type Ext<'t> = ();
    type Sql = i64;
}

impl SecretFromSql for i64 {
    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        value.as_i64()
    }
}

impl MyTyp for f64 {
    type Prev = Self;
    const TYP: hash::ColumnType = hash::ColumnType::Float;
    type Out = Self;
    type Ext<'t> = ();
    type Sql = f64;
}

impl SecretFromSql for f64 {
    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        value.as_f64()
    }
}

impl MyTyp for bool {
    type Prev = Self;
    const TYP: hash::ColumnType = hash::ColumnType::Integer;
    type Out = Self;
    type Ext<'t> = ();
    type Sql = bool;
}

impl SecretFromSql for bool {
    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(value.as_i64()? != 0)
    }
}

impl MyTyp for String {
    type Prev = Self;
    const TYP: hash::ColumnType = hash::ColumnType::String;
    type Out = Self;
    type Ext<'t> = ();
    type Sql = String;
}
assert_impl_all!(String: Nullable);

impl SecretFromSql for String {
    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(value.as_str()?.to_owned())
    }
}

impl MyTyp for Vec<u8> {
    type Prev = Self;
    const TYP: hash::ColumnType = hash::ColumnType::Blob;
    type Out = Self;
    type Ext<'t> = ();
    type Sql = Vec<u8>;
}
assert_impl_all!(Vec<u8>: Nullable);

impl SecretFromSql for Vec<u8> {
    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(value.as_blob()?.to_owned())
    }
}

impl<T: MyTyp> MyTyp for Option<T> {
    type Prev = Option<T::Prev>;
    const TYP: hash::ColumnType = T::TYP;
    const NULLABLE: bool = true;
    const FK: Option<(&'static str, &'static str)> = T::FK;
    type Out = Option<T::Out>;
    type Ext<'t> = ();
    type Sql = T::Sql;
}

impl<T: SecretFromSql> SecretFromSql for Option<T> {
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
/// [Expr] implements [Deref] to have column fields in case the expression has a table type.
pub struct Expr<'column, S, T: MyTyp> {
    pub(crate) _local: PhantomData<*const ()>,
    pub(crate) inner: DynTyped<T>,
    pub(crate) _p: PhantomData<&'column ()>,
    pub(crate) _p2: PhantomData<S>,
    pub(crate) ext: OnceCell<Box<T::Ext<'static>>>,
}

impl<S, T: MyTyp> Debug for Expr<'_, S, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Expr of type {}", std::any::type_name::<T>())
    }
}

impl<'column, S, T: MyTyp> Expr<'column, S, T> {
    /// Extremely easy to use API. Should only be used by the macro to implement migrations.
    #[doc(hidden)]
    pub fn _migrate<OldS>(prev: impl IntoExpr<'column, OldS>) -> Self {
        Self::new(MigratedExpr {
            prev: prev.into_expr().inner.erase(),
            _p: PhantomData,
        })
    }
}

pub fn adhoc_expr<S, T: MyTyp>(
    f: impl 'static + Fn(&mut ValueBuilder) -> sea_query::Expr,
) -> Expr<'static, S, T> {
    Expr::adhoc(f)
}

pub fn new_column<'x, S, C: MyTyp, T: Table>(
    table: impl IntoExpr<'x, S, Typ = T>,
    name: &'static str,
) -> Expr<'x, S, C> {
    let table = table.into_expr().inner;
    Expr::adhoc(move |b| sea_query::Expr::col((table.build_table(b), Field::Str(name))).into())
}

pub fn assume_expr<S, T: MyTyp>(e: Expr<S, Option<T>>) -> Expr<S, T> {
    let inner = e.inner;
    Expr::adhoc(move |b| inner.build_expr(b))
}

pub fn new_dummy<'x, S, T: MyTyp>(val: impl Typed<Typ = T> + 'static) -> Select<'x, S, T::Out> {
    IntoSelect::into_select(Expr::new(val))
}

pub fn into_owned<'x, S, T: MyTyp>(val: impl IntoExpr<'x, S, Typ = T>) -> DynTyped<T> {
    val.into_expr().inner
}

struct AdHoc<F, T>(F, PhantomData<T>);
impl<F: Fn(&mut ValueBuilder) -> sea_query::Expr, T> Typed for AdHoc<F, T> {
    type Typ = T;

    fn build_expr(&self, b: &mut ValueBuilder) -> sea_query::Expr {
        (self.0)(b)
    }
}

impl<S, T: MyTyp> Expr<'_, S, T> {
    pub(crate) fn adhoc(f: impl 'static + Fn(&mut ValueBuilder) -> sea_query::Expr) -> Self {
        Self::new(AdHoc(f, PhantomData))
    }

    pub(crate) fn new(val: impl Typed<Typ = T> + 'static) -> Self {
        Self {
            _local: PhantomData,
            inner: DynTyped(Rc::new(val)),
            _p: PhantomData,
            _p2: PhantomData,
            ext: OnceCell::new(),
        }
    }
}

impl<S, T: MyTyp> Clone for Expr<'_, S, T> {
    fn clone(&self) -> Self {
        Self {
            _local: PhantomData,
            inner: self.inner.clone(),
            _p: self._p,
            _p2: self._p2,
            ext: OnceCell::new(),
        }
    }
}

#[derive(Clone)]
pub struct DynTypedExpr(pub(crate) Rc<dyn Fn(&mut ValueBuilder) -> sea_query::Expr>);

impl DynTypedExpr {
    pub fn new(f: impl 'static + Fn(&mut ValueBuilder) -> sea_query::Expr) -> Self {
        Self(Rc::new(f))
    }
}

impl<Typ: 'static> DynTyped<Typ> {
    pub fn erase(self) -> DynTypedExpr {
        DynTypedExpr(Rc::new(move |b| self.build_expr(b)))
    }
}

pub struct MigratedExpr<Typ> {
    prev: DynTypedExpr,
    _p: PhantomData<Typ>,
}

impl<Typ> Typed for MigratedExpr<Typ> {
    type Typ = Typ;
    fn build_expr(&self, b: &mut ValueBuilder) -> sea_query::Expr {
        self.prev.0(b)
    }
}

pub struct DynTyped<Typ>(pub(crate) Rc<dyn Typed<Typ = Typ>>);

impl<Typ> DynTyped<Typ> {
    pub fn new(val: impl 'static + Typed<Typ = Typ>) -> Self {
        Self(Rc::new(val))
    }
}

impl<T> Clone for DynTyped<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<Typ: 'static> Typed for DynTyped<Typ> {
    type Typ = Typ;

    fn build_expr(&self, b: &mut ValueBuilder) -> sea_query::Expr {
        self.0.build_expr(b)
    }

    fn build_table(&self, b: &mut ValueBuilder) -> MyAlias
    where
        Self::Typ: Table,
    {
        self.0.build_table(b)
    }
}

impl<'column, S, T: MyTyp> IntoExpr<'column, S> for Expr<'column, S, T> {
    type Typ = T;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        self
    }
}

impl<'t, T: Table> Deref for Expr<'t, T::Schema, T> {
    type Target = T::Ext2<'t>;

    fn deref(&self) -> &Self::Target {
        T::covariant_ext(self.ext.get_or_init(|| {
            let expr = Expr {
                _local: PhantomData,
                inner: self.inner.clone(),
                _p: PhantomData::<&'static ()>,
                _p2: PhantomData,
                ext: OnceCell::new(),
            };
            Box::new(T::build_ext2(&expr))
        }))
    }
}
