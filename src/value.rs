use std::{
    cell::OnceCell,
    ops::Deref,
    sync::atomic::{AtomicU64, Ordering},
};

use elsa::FrozenVec;
use sea_query::{Expr, Iden, IntoColumnRef, SimpleExpr};

use crate::{
    ast::{Joins, MyTable},
    Builder, HasId,
};

pub trait Value<'t>: Sized {
    type Typ: MyIdenT;
    fn build_expr(&self) -> SimpleExpr;

    fn add<T: Value<'t>>(self, rhs: T) -> MyAdd<Self, T> {
        MyAdd(self, rhs)
    }

    fn lt(self, rhs: i32) -> MyLt<Self> {
        MyLt(self, rhs)
    }

    fn eq<T: Value<'t>>(self, rhs: T) -> MyEq<Self, T> {
        MyEq(self, rhs)
    }

    fn not(self) -> MyNot<Self> {
        MyNot(self)
    }
}

impl<'t, T: Value<'t>> Value<'t> for &'_ T {
    type Typ = T::Typ;

    fn build_expr(&self) -> SimpleExpr {
        T::build_expr(self)
    }
}

impl<'t, T: MyIdenT> Value<'t> for Db<'t, T> {
    type Typ = T;
    fn build_expr(&self) -> SimpleExpr {
        Expr::col(self.info.alias()).into()
    }
}

#[derive(Clone, Copy)]
pub struct MyAdd<A, B>(A, B);

impl<'t, A: Value<'t>, B: Value<'t>> Value<'t> for MyAdd<A, B> {
    type Typ = A::Typ;
    fn build_expr(&self) -> SimpleExpr {
        self.0.build_expr().add(self.1.build_expr())
    }
}

#[derive(Clone, Copy)]
pub struct MyNot<T>(T);

impl<'t, T: Value<'t>> Value<'t> for MyNot<T> {
    type Typ = T::Typ;
    fn build_expr(&self) -> SimpleExpr {
        self.0.build_expr().not()
    }
}

#[derive(Clone, Copy)]
pub struct MyLt<A>(A, i32);

impl<'t, A: Value<'t>> Value<'t> for MyLt<A> {
    type Typ = bool;
    fn build_expr(&self) -> SimpleExpr {
        Expr::expr(self.0.build_expr()).lt(self.1)
    }
}

#[derive(Clone, Copy)]
pub struct MyEq<A, B>(A, B);

impl<'t, A: Value<'t>, B: Value<'t>> Value<'t> for MyEq<A, B> {
    type Typ = bool;
    fn build_expr(&self) -> SimpleExpr {
        self.0.build_expr().eq(self.1.build_expr())
    }
}

#[derive(Clone, Copy)]
pub struct Const<T>(pub T);

impl<'t, T: MyIdenT> Value<'t> for Const<T>
where
    T: Into<sea_query::value::Value> + Clone,
{
    type Typ = T;
    fn build_expr(&self) -> SimpleExpr {
        SimpleExpr::from(self.0.clone())
    }
}

#[derive(Clone, Copy)]
pub struct Unwrapped<T>(pub(crate) T);

impl<'t, T: MyIdenT, A: Value<'t, Typ = Option<T>>> Value<'t> for Unwrapped<A> {
    type Typ = T;
    fn build_expr(&self) -> SimpleExpr {
        A::build_expr(&self.0)
    }
}

#[derive(Clone, Copy)]
pub struct UnwrapOr<T>(pub(crate) T, pub(crate) i64);

impl<'t, A: Value<'t, Typ = Option<i64>>> Value<'t> for UnwrapOr<A> {
    type Typ = i64;
    fn build_expr(&self) -> SimpleExpr {
        Expr::expr(A::build_expr(&self.0)).if_null(self.1)
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct FieldAlias {
    pub table: MyAlias,
    pub col: Field,
}

impl IntoColumnRef for FieldAlias {
    fn into_column_ref(self) -> sea_query::ColumnRef {
        (self.table, self.col).into_column_ref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Field {
    U64(MyAlias),
    Str(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct MyAlias {
    name: u64,
}

impl sea_query::Iden for Field {
    fn unquoted(&self, s: &mut dyn std::fmt::Write) {
        match self {
            Field::U64(alias) => alias.unquoted(s),
            Field::Str(name) => write!(s, "{}", name).unwrap(),
        }
    }
    // TODO: remove
    fn prepare(&self, s: &mut dyn std::fmt::Write, _q: sea_query::Quote) {
        self.unquoted(s)
    }
}

impl MyAlias {
    pub fn new() -> Self {
        static IDEN_NUM: AtomicU64 = AtomicU64::new(0);
        let next = IDEN_NUM.fetch_add(1, Ordering::Relaxed);
        Self { name: next }
    }
}

impl sea_query::Iden for MyAlias {
    fn unquoted(&self, s: &mut dyn std::fmt::Write) {
        write!(s, "_{}", self.name).unwrap()
    }
    // TODO: remove
    fn prepare(&self, s: &mut dyn std::fmt::Write, _q: sea_query::Quote) {
        self.unquoted(s)
    }
}

pub(super) trait MyTableT<'t> {
    fn unwrap(joined: &'t FrozenVec<Box<(Field, MyTable)>>, field: FieldAlias) -> Self;
    fn alias(&self) -> FieldAlias;
}

impl<'t, T: HasId> MyTableT<'t> for FkInfo<'t, T> {
    fn unwrap(joined: &'t FrozenVec<Box<(Field, MyTable)>>, field: FieldAlias) -> Self {
        FkInfo {
            field,
            joined,
            inner: OnceCell::new(),
        }
    }
    fn alias(&self) -> FieldAlias {
        self.field
    }
}

impl<'t> MyTableT<'t> for ValueInfo {
    fn unwrap(_joined: &'t FrozenVec<Box<(Field, MyTable)>>, field: FieldAlias) -> Self {
        ValueInfo { field }
    }
    fn alias(&self) -> FieldAlias {
        self.field
    }
}

pub(super) struct FkInfo<'t, T: HasId> {
    pub field: FieldAlias,
    pub joined: &'t FrozenVec<Box<(Field, MyTable)>>, // the table that we join onto
    pub inner: OnceCell<Box<T::Dummy<'t>>>,
}

impl<'t, T: HasId> FkInfo<'t, T> {
    pub(crate) fn joined(
        joined: &'t FrozenVec<Box<(Field, MyTable)>>,
        field: FieldAlias,
    ) -> Db<'t, T> {
        Db {
            info: FkInfo {
                field,
                joined,
                // prevent unnecessary join
                inner: OnceCell::from(Box::new(T::build(Builder::new_full(joined, field.table)))),
            },
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ValueInfo {
    pub field: FieldAlias,
}

pub(super) trait MyIdenT: Sized {
    type Info<'t>: MyTableT<'t>;
    fn iden_any(joins: &Joins, col: Field) -> Db<'_, Self> {
        let field = FieldAlias {
            table: joins.table,
            col,
        };
        Self::iden_full(&joins.joined, field)
    }
    fn iden_full(joined: &FrozenVec<Box<(Field, MyTable)>>, field: FieldAlias) -> Db<'_, Self> {
        Db {
            info: Self::Info::unwrap(joined, field),
        }
    }
}

impl<T: HasId> MyIdenT for T {
    type Info<'t> = FkInfo<'t, T>;
}

impl MyIdenT for i64 {
    type Info<'t> = ValueInfo;
}

impl MyIdenT for bool {
    type Info<'t> = ValueInfo;
}

impl MyIdenT for String {
    type Info<'t> = ValueInfo;
}

impl<T: MyIdenT> MyIdenT for Option<T> {
    type Info<'t> = T::Info<'t>;
}

// invariant in `'t` because of the associated type
pub struct Db<'t, T: MyIdenT> {
    pub(super) info: T::Info<'t>,
}

impl<'t, T: MyIdenT> Clone for Db<'t, T>
where
    T::Info<'t>: Clone,
{
    fn clone(&self) -> Self {
        Db {
            info: self.info.clone(),
        }
    }
}
impl<'t, T: MyIdenT> Copy for Db<'t, T> where T::Info<'t>: Copy {}

impl<'a, T: HasId> Db<'a, T> {
    pub fn id(&self) -> Db<'a, i64> {
        Db {
            info: ValueInfo {
                field: self.info.field,
            },
        }
    }
}

impl<'a, T: HasId> Deref for Db<'a, T> {
    type Target = T::Dummy<'a>;

    fn deref(&self) -> &Self::Target {
        self.info.inner.get_or_init(|| {
            let joined = self.info.joined;
            let name = self.info.field.col;
            let table = if let Some(item) = joined.iter().find(|item| item.0 == name) {
                &item.1
            } else {
                let table = MyTable {
                    name: T::NAME,
                    id: T::ID,
                    joins: Joins {
                        table: MyAlias::new(),
                        joined: FrozenVec::new(),
                    },
                };
                &joined.push_get(Box::new((name, table))).1
            };

            Box::new(T::build(Builder::new(&table.joins)))
        })
    }
}

pub(crate) struct RawAlias(pub(crate) String);

impl Iden for RawAlias {
    fn unquoted(&self, s: &mut dyn std::fmt::Write) {
        write!(s, "{}", self.0).unwrap()
    }
    fn prepare(&self, s: &mut dyn std::fmt::Write, _q: sea_query::Quote) {
        self.unquoted(s)
    }
}
