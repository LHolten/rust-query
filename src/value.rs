use std::{
    cell::OnceCell,
    ops::Deref,
    sync::atomic::{AtomicU64, Ordering},
};

use elsa::FrozenVec;
use sea_query::{Expr, IntoColumnRef, SimpleExpr};

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
    T: Into<sea_query::value::Value> + Copy,
{
    type Typ = T;
    fn build_expr(&self) -> SimpleExpr {
        SimpleExpr::from(self.0)
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
    fn unwrap(val: &'t Joins, field: Field) -> Self;
    fn alias(&self) -> FieldAlias;
}

impl<'t, T: HasId> MyTableT<'t> for FkInfo<'t, T> {
    fn unwrap(table: &'t Joins, field: Field) -> Self {
        FkInfo {
            field,
            joins: table,
            inner: OnceCell::new(),
        }
    }
    fn alias(&self) -> FieldAlias {
        FieldAlias {
            table: self.joins.alias,
            col: self.field,
        }
    }
}

impl<'t> MyTableT<'t> for ValueInfo {
    fn unwrap(table: &'t Joins, field: Field) -> Self {
        ValueInfo {
            field: FieldAlias {
                table: table.alias,
                col: field,
            },
        }
    }
    fn alias(&self) -> FieldAlias {
        self.field
    }
}

pub(super) struct FkInfo<'t, T: HasId> {
    pub field: Field,
    pub joins: &'t Joins, // the table that we join onto
    pub inner: OnceCell<Box<T::Dummy<'t>>>,
}

#[derive(Clone, Copy)]
pub(super) struct ValueInfo {
    pub field: FieldAlias,
}

pub(super) trait MyIdenT: Sized {
    type Info<'t>: MyTableT<'t>;
    fn iden_any(joins: &Joins, field: Field) -> Db<'_, Self> {
        Db {
            info: Self::Info::unwrap(joins, field),
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
                field: FieldAlias {
                    table: self.info.joins.alias,
                    col: self.info.field,
                },
            },
        }
    }
}

impl<'a, T: HasId> Deref for Db<'a, T> {
    type Target = T::Dummy<'a>;

    fn deref(&self) -> &Self::Target {
        self.info.inner.get_or_init(|| {
            let t = self.info.joins;
            let name = self.info.field;
            let table = if let Some(item) = t.joined.iter().find(|item| item.0 == name) {
                &item.1
            } else {
                let table = MyTable {
                    name: T::NAME,
                    id: T::ID,
                    joins: Joins {
                        alias: MyAlias::new(),
                        joined: FrozenVec::new(),
                    },
                };
                &t.joined.push_get(Box::new((name, table))).1
            };

            Box::new(T::build(Builder::new(&table.joins)))
        })
    }
}
