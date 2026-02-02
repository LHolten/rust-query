pub mod aggregate;
mod db_typ;
mod operations;
pub mod optional;
pub mod trivial;

use std::{cell::OnceCell, fmt::Debug, marker::PhantomData, ops::Deref, rc::Rc};

use sea_query::{Alias, JoinType, Nullable, SelectStatement};

use crate::{
    IntoSelect, Select, Table,
    alias::{Field, JoinableTable, MyAlias, Scope},
    ast::{MySelect, Source},
    db::TableRow,
    mutable::Mutable,
    mymap::MyMap,
    private::IntoJoinable,
};
pub use db_typ::DbTyp;

#[derive(Default)]
pub struct ValueBuilder {
    pub(crate) from: Rc<MySelect>,
    // only used for tables
    pub(super) scope: Scope,
    // implicit joins
    pub(super) extra: MyMap<Source, MyAlias>,
    // calculating these results
    pub(super) forwarded: MyMap<MyTableRef, MyAlias>,
}

impl ValueBuilder {
    pub(crate) fn get_aggr(
        &mut self,
        aggr: Rc<SelectStatement>,
        conds: Vec<MyTableRef>,
    ) -> MyAlias {
        let source = Source {
            kind: crate::ast::SourceKind::Aggregate(aggr),
            conds: conds
                .into_iter()
                .enumerate()
                .map(|(idx, join)| {
                    let alias = Alias::new(join.table_name.main_column());
                    (
                        Field::U64(MyAlias::new(idx)),
                        sea_query::Expr::col((self.get_table(join), alias)),
                    )
                })
                .collect(),
        };
        let new_alias = || self.scope.new_alias();
        *self.extra.get_or_init(source, new_alias)
    }

    pub(crate) fn get_join<T: Table>(
        &mut self,
        expr: sea_query::Expr,
        possible_null: bool,
        new_col: &'static str,
    ) -> sea_query::Expr {
        match &expr {
            // we could use our own type instead of `sea_query::Expr` to make it much easier
            // to know if we joined the table explicitly, but that would add much complexity
            // everywhere else
            sea_query::Expr::Column(sea_query::ColumnRef::Column(sea_query::ColumnName(
                Some(sea_query::TableName(None, table)),
                col,
            ))) => {
                // check if this table has been joined explicitly
                if let Some(alias) = MyAlias::try_from(table)
                    && let Some(from) = self.from.tables.get(alias.idx)
                    && from.main_column() == col.inner().as_ref()
                {
                    // No need to join the table again
                    return sea_query::Expr::col((alias, new_col));
                }
            }
            _ => (),
        };

        let join_type = if possible_null {
            JoinType::LeftJoin
        } else {
            JoinType::Join
        };
        let source = Source {
            kind: crate::ast::SourceKind::Implicit(T::NAME.to_owned(), join_type),
            conds: vec![(Field::Str(T::ID), expr)],
        };
        let new_alias = || self.scope.new_alias();

        // TODO: possible optimization to unify the join_type?
        // e.g. join + left join = join
        let alias = *self.extra.get_or_init(source, new_alias);

        sea_query::Expr::col((alias, new_col))
    }

    pub fn get_unique<T: Table>(
        &mut self,
        conds: Box<[(&'static str, sea_query::Expr)]>,
    ) -> sea_query::Expr {
        let source = Source {
            kind: crate::ast::SourceKind::Implicit(T::NAME.to_owned(), JoinType::LeftJoin),
            conds: conds.into_iter().map(|x| (Field::Str(x.0), x.1)).collect(),
        };

        let new_alias = || self.scope.new_alias();
        let table = self.extra.get_or_init(source, new_alias);
        sea_query::Expr::col((*table, Alias::new(T::ID))).into()
    }

    pub fn get_table(&mut self, table: MyTableRef) -> MyAlias {
        if Rc::ptr_eq(&self.from.scope_rc, &table.scope_rc) {
            MyAlias::new(table.idx)
        } else {
            *self.forwarded.get_or_init(table, || self.scope.new_alias())
        }
    }
}

/// This references a particular user specified join,
/// so not any of the forwarded joins.
/// We use this to know if the current scope has the original join or needs to forward it.
#[derive(Clone)]
pub struct MyTableRef {
    // one Rc exists for each scope, so we can check if we have the right
    // scope by comparing the Rc ptr.
    pub(crate) scope_rc: Rc<()>,
    pub(crate) idx: usize,
    pub(crate) table_name: JoinableTable,
}

impl PartialEq for MyTableRef {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.scope_rc, &other.scope_rc) && self.idx == other.idx
    }
}

pub trait NumTyp: OrdTyp + Clone + Copy {
    const ZERO: sea_query::Value;
}

impl NumTyp for i64 {
    const ZERO: sea_query::Value = sea_query::Value::BigInt(Some(0));
}
impl NumTyp for f64 {
    const ZERO: sea_query::Value = sea_query::Value::Double(Some(0.));
}

pub trait OrdTyp: EqTyp {}
impl OrdTyp for String {}
impl OrdTyp for Vec<u8> {}
impl OrdTyp for i64 {}
impl OrdTyp for f64 {}
impl OrdTyp for bool {}

pub trait BuffTyp: DbTyp {}
impl BuffTyp for String {}
impl BuffTyp for Vec<u8> {}

#[diagnostic::on_unimplemented(
    message = "Columns with type `{Self}` can not be checked for equality",
    note = "`EqTyp` is also implemented for all table types"
)]
pub trait EqTyp: DbTyp {}

impl EqTyp for String {}
impl EqTyp for Vec<u8> {}
impl EqTyp for i64 {}
impl EqTyp for f64 {}
impl EqTyp for bool {}
#[diagnostic::do_not_recommend]
impl<T: Table> EqTyp for TableRow<T> {}

/// Trait for all values that can be used as expressions in queries.
///
/// There is a hierarchy of types that can be used to build queries.
/// - [TableRow], [i64], [f64], [bool], `&[u8]`, `&str`:
///   These are the base types for building expressions. They all
///   implement [IntoExpr] and are [Copy]. Note that [TableRow] is special
///   because it refers to a table row that is guaranteed to exist.
/// - [Expr] is the type that all [IntoExpr] values can be converted into.
///   It has a lot of methods to combine expressions into more complicated expressions.
///   Next to those, it implements [std::ops::Deref], if the expression is a table expression.
///   This can be used to get access to the columns of the table, which can themselves be table expressions.
///   Note that combinators like [crate::optional] and [crate::aggregate] also have [Expr] as return type.
///
/// Note that while [Expr] implements [IntoExpr], you may want to use `&Expr` instead.
/// Using a reference lets you reuse [Expr] without calling [Clone] explicitly.
pub trait IntoExpr<'column, S> {
    /// The type of the expression.
    type Typ: DbTyp;

    /// Turn this value into an [Expr].
    fn into_expr(self) -> Expr<'column, S, Self::Typ>;
}

impl<'column, S, T: IntoExpr<'column, S, Typ = X>, X: EqTyp> IntoExpr<'column, S> for Option<T> {
    type Typ = Option<X>;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        let this = self.map(|x| x.into_expr().inner);
        Expr::adhoc(move |b| {
            this.as_ref()
                .map(|x| (x.func)(b))
                .unwrap_or(<X::Sql as Nullable>::null().into())
        })
    }
}

impl<'column, S> IntoExpr<'column, S> for String {
    type Typ = String;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::adhoc(move |_| sea_query::Expr::from(self.clone()))
    }
}

impl<'column, S> IntoExpr<'column, S> for &str {
    type Typ = String;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        self.to_owned().into_expr()
    }
}

impl<'column, S> IntoExpr<'column, S> for Vec<u8> {
    type Typ = Vec<u8>;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::adhoc(move |_| sea_query::Expr::from(self.clone()))
    }
}

impl<'column, S> IntoExpr<'column, S> for &[u8] {
    type Typ = Vec<u8>;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        self.to_owned().into_expr()
    }
}

impl<'column, S> IntoExpr<'column, S> for bool {
    type Typ = bool;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::adhoc(move |_| sea_query::Expr::from(self))
    }
}

impl<'column, S> IntoExpr<'column, S> for i64 {
    type Typ = i64;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::adhoc(move |_| sea_query::Expr::from(self))
    }
}
impl<'column, S> IntoExpr<'column, S> for f64 {
    type Typ = f64;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::adhoc(move |_| sea_query::Expr::from(self))
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

pub trait OptTable: DbTyp {
    type Schema;
    type Select;
    type Mutable<'t>;
    fn select_opt_mutable(
        val: Expr<'_, Self::Schema, Self>,
    ) -> Select<'_, Self::Schema, Self::Select>;

    fn into_mutable<'t>(val: Self::Select) -> Self::Mutable<'t>;
}

impl<T: Table> OptTable for TableRow<T> {
    type Schema = T::Schema;
    type Select = (T::Select, TableRow<T>);
    type Mutable<'t> = Mutable<'t, T>;
    fn select_opt_mutable(
        val: Expr<'_, Self::Schema, Self>,
    ) -> Select<'_, Self::Schema, Self::Select> {
        (T::into_select(val.clone()), val).into_select()
    }

    fn into_mutable<'t>((inner, row_id): Self::Select) -> Self::Mutable<'t> {
        Mutable::new(T::select_mutable(inner), row_id)
    }
}

impl<T: Table> OptTable for Option<TableRow<T>> {
    type Schema = T::Schema;
    type Select = Option<(T::Select, TableRow<T>)>;
    type Mutable<'t> = Option<Mutable<'t, T>>;
    fn select_opt_mutable(
        val: Expr<'_, Self::Schema, Self>,
    ) -> Select<'_, Self::Schema, Self::Select> {
        crate::optional(|row| {
            let val = row.and(val);
            row.then_select((T::into_select(val.clone()), val))
        })
    }

    fn into_mutable<'t>(val: Self::Select) -> Self::Mutable<'t> {
        val.map(TableRow::<T>::into_mutable)
    }
}

/// This is an expression that can be used in queries.
///
/// - The lifetime parameter `'column` specifies which columns need to be in scope.
/// - The type parameter `S` specifies the expected schema of the query.
/// - And finally the type paramter `T` specifies the type of the expression.
///
/// [Expr] implements [Deref] to have column fields in case the expression has a table type.
pub struct Expr<'column, S, T: DbTyp + ?Sized> {
    pub(crate) _local: PhantomData<*const ()>,
    pub(crate) inner: Rc<AdHoc<dyn Fn(&mut ValueBuilder) -> sea_query::Expr, T>>,
    pub(crate) _p: PhantomData<&'column ()>,
    pub(crate) _p2: PhantomData<S>,
    pub(crate) ext: OnceCell<Box<T::Ext<'static>>>,
}

impl<S, T: DbTyp> Debug for Expr<'_, S, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Expr of type {}", std::any::type_name::<T>())
    }
}

impl<'column, S, T: DbTyp> Expr<'column, S, T> {
    /// Extremely easy to use API. Should only be used by the macro to implement migrations.
    #[doc(hidden)]
    pub fn _migrate<OldS>(prev: impl IntoExpr<'column, OldS>) -> Self {
        let prev = DynTypedExpr::erase(prev);
        Self::adhoc(move |b| (prev.func)(b))
    }
}

pub fn adhoc_expr<S, T: DbTyp>(
    f: impl 'static + Fn(&mut ValueBuilder) -> sea_query::Expr,
) -> Expr<'static, S, T> {
    Expr::adhoc(f)
}

pub fn new_column<'x, S, C: DbTyp, T: Table>(
    table: impl IntoExpr<'x, S, Typ = TableRow<T>>,
    name: &'static str,
) -> Expr<'x, S, C> {
    let table = table.into_expr().inner;
    let possible_null = table.maybe_optional;
    Expr::adhoc_promise(
        move |b| {
            let main_column = table.build_expr(b);
            b.get_join::<T>(main_column, table.maybe_optional, name)
        },
        possible_null,
    )
}

pub fn unique_from_joinable<'inner, T: Table>(
    j: impl IntoJoinable<'inner, T::Schema, Typ = TableRow<T>>,
) -> Expr<'inner, T::Schema, Option<TableRow<T>>> {
    let list = j.into_joinable().conds;
    ::rust_query::private::adhoc_expr(move |_b| {
        let list = list
            .iter()
            .map(|(name, col)| (*name, (col.func)(_b)))
            .collect();
        _b.get_unique::<T>(list)
    })
}

pub struct AdHoc<F: ?Sized, T: ?Sized> {
    maybe_optional: bool,
    _p: PhantomData<T>,
    func: F,
}

impl<F: ?Sized + Fn(&mut ValueBuilder) -> sea_query::Expr, T> AdHoc<F, T> {
    pub fn build_expr(&self, b: &mut ValueBuilder) -> sea_query::Expr {
        (self.func)(b)
    }
}

impl<S, T: DbTyp> Expr<'_, S, T> {
    pub(crate) fn adhoc(f: impl 'static + Fn(&mut ValueBuilder) -> sea_query::Expr) -> Self {
        Self::adhoc_promise(f, true)
    }

    /// Only set `maybe_optional` to `false` if you are absolutely sure that the
    /// value is not null. The [crate::optional] combinator makes this more difficult.
    /// There is no reason to use this for values that can not be foreign keys.
    /// This is used to optimize implicit joins from LEFT JOIN to just JOIN.
    pub(crate) fn adhoc_promise(
        f: impl 'static + Fn(&mut ValueBuilder) -> sea_query::Expr,
        maybe_optional: bool,
    ) -> Self {
        Self::new(Rc::new(AdHoc {
            func: f,
            maybe_optional,
            _p: PhantomData,
        }))
    }

    pub(crate) fn new(val: Rc<AdHoc<dyn Fn(&mut ValueBuilder) -> sea_query::Expr, T>>) -> Self {
        Self {
            _local: PhantomData,
            inner: val,
            _p: PhantomData,
            _p2: PhantomData,
            ext: OnceCell::new(),
        }
    }
}

impl<S, T: DbTyp> Clone for Expr<'_, S, T> {
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
pub struct DynTypedExpr {
    pub func: Rc<dyn Fn(&mut ValueBuilder) -> sea_query::Expr>,
}

impl DynTypedExpr {
    pub fn new(f: impl 'static + Fn(&mut ValueBuilder) -> sea_query::Expr) -> Self {
        Self { func: Rc::new(f) }
    }
    pub fn erase<'x, S>(expr: impl IntoExpr<'x, S>) -> Self {
        let typed = expr.into_expr().inner;
        Self::new(move |b| typed.build_expr(b))
    }
}

impl<'column, S, T: DbTyp> IntoExpr<'column, S> for Expr<'column, S, T> {
    type Typ = T;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        self
    }
}

impl<'t, T: Table> Deref for Expr<'t, T::Schema, TableRow<T>> {
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
