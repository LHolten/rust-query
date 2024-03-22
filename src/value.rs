use std::{
    cell::OnceCell,
    ops::Deref,
    sync::atomic::{AtomicU64, Ordering},
};

use elsa::FrozenVec;
use sea_query::{Alias, Expr, SimpleExpr};

use crate::{ast::MyTable, Builder, Table};

pub trait Value: Sized {
    type Typ: MyIdenT;
    fn into_expr(self) -> SimpleExpr;

    fn add<T: Value>(self, rhs: T) -> MyAdd<Self, T> {
        MyAdd(self, rhs)
    }

    fn lt(self, rhs: i32) -> MyLt<Self> {
        MyLt(self, rhs)
    }

    fn eq<T: Value>(self, rhs: T) -> MyEq<Self, T> {
        MyEq(self, rhs)
    }

    fn not(self) -> MyNot<Self> {
        MyNot(self)
    }
}

// impl<'t, A: Value, B: Value> Value for (A, B) {
//     type Typ = (A::Typ, B::Typ);
//     fn into_expr(self) -> SimpleExpr {
//         Expr::tuple([self.0.into_expr(), self.1.into_expr()]).into()
//     }
// }

impl<'t, T: MyIdenT> Value for Db<'t, T> {
    type Typ = T;
    fn into_expr(self) -> SimpleExpr {
        Expr::col(self.col.alias()).into()
    }
}

#[derive(Clone, Copy)]
pub struct MyAdd<A, B>(A, B);

impl<'t, A: Value, B: Value> Value for MyAdd<A, B> {
    type Typ = A::Typ;
    fn into_expr(self) -> SimpleExpr {
        self.0.into_expr().add(self.1.into_expr())
    }
}

#[derive(Clone, Copy)]
pub struct MyNot<T>(T);

impl<'t, T: Value> Value for MyNot<T> {
    type Typ = T::Typ;
    fn into_expr(self) -> SimpleExpr {
        self.0.into_expr().not()
    }
}

#[derive(Clone, Copy)]
pub struct MyLt<A>(A, i32);

impl<'t, A: Value> Value for MyLt<A> {
    type Typ = bool;
    fn into_expr(self) -> SimpleExpr {
        Expr::expr(self.0.into_expr()).lt(self.1)
    }
}

#[derive(Clone, Copy)]
pub struct MyEq<A, B>(A, B);

impl<'t, A: Value, B: Value> Value for MyEq<A, B> {
    type Typ = bool;
    fn into_expr(self) -> SimpleExpr {
        self.0.into_expr().eq(self.1.into_expr())
    }
}

#[derive(Clone, Copy)]
pub struct Const<T>(pub T);

impl<'t, T: MyIdenT> Value for Const<T>
where
    T: Into<sea_query::value::Value> + Copy,
{
    type Typ = T;
    fn into_expr(self) -> SimpleExpr {
        SimpleExpr::from(self.0)
    }
}

#[derive(Clone, Copy)]
pub(super) struct MyAlias {
    name: u64,
}

impl MyAlias {
    pub fn new() -> Self {
        static IDEN_NUM: AtomicU64 = AtomicU64::new(0);
        let next = IDEN_NUM.fetch_add(1, Ordering::Relaxed);
        Self { name: next }
    }

    pub fn iden<'t, T: MyIdenT<Alias = Self, Info<'t> = ()>>(&'t self) -> Db<'t, T> {
        Db {
            col: self,
            info: (),
        }
    }

    pub fn into_alias(&self) -> Alias {
        Alias::new(format!("{}", self.name))
    }
}

pub struct MyTableAlias {
    pub(super) val: MyAlias,
    pub(super) table: MyTable,
}

impl MyTableAlias {
    pub(crate) fn new(table: &'static str) -> MyTableAlias {
        MyTableAlias {
            val: MyAlias::new(),
            table: MyTable {
                name: table,
                columns: FrozenVec::new(),
            },
        }
    }

    pub fn fk<'t, T: Table>(&'t self) -> Db<'t, T> {
        Db {
            col: &self,
            info: TableInfo {
                table: &self.table,
                inner: OnceCell::new(),
            },
        }
    }
}

pub(super) enum AnyAlias {
    Value(MyAlias),
    Table(MyTableAlias),
}

impl AnyAlias {
    pub fn as_val(&self) -> &MyAlias {
        match self {
            AnyAlias::Value(val) => val,
            AnyAlias::Table(_) => todo!(),
        }
    }

    pub fn as_table(&self) -> &MyTableAlias {
        match self {
            AnyAlias::Value(_) => todo!(),
            AnyAlias::Table(table) => table,
        }
    }
}

impl AnyAlias {
    pub fn into_alias(&self) -> Alias {
        match self {
            AnyAlias::Value(x) => x.into_alias(),
            AnyAlias::Table(x) => x.val.into_alias(),
        }
    }
}

pub trait MyAliasT {
    fn alias(&self) -> Alias;
    // fn unwrap(val: AnyAlias) -> Self;
}

impl MyAliasT for MyAlias {
    fn alias(&self) -> Alias {
        self.into_alias()
    }
    // fn unwrap(val: AnyAlias) -> Self {
    //     match val {
    //         AnyAlias::Value(val) => val,
    //         AnyAlias::Table(_) => panic!(),
    //     }
    // }
}

impl MyAliasT for MyTableAlias {
    fn alias(&self) -> Alias {
        self.val.into_alias()
    }
    // fn unwrap(val: AnyAlias) -> Self {
    //     match val {
    //         AnyAlias::Value(_) => panic!(),
    //         AnyAlias::Table(table) => table,
    //     }
    // }
}

pub(super) trait MyIdenT: Sized {
    type Alias: MyAliasT;
    type Info<'t>;
    fn new_alias() -> AnyAlias;
    fn iden(col: &AnyAlias) -> Db<'_, Self>;
}

pub(super) struct TableInfo<'t, T: Table> {
    pub table: &'t MyTable,
    pub inner: OnceCell<T::Dummy<'t>>,
}

impl<T: Table> MyIdenT for T {
    type Alias = MyTableAlias;
    type Info<'t> = TableInfo<'t, T>;
    fn new_alias() -> AnyAlias {
        AnyAlias::Table(MyTableAlias::new(T::NAME))
    }
    fn iden(col: &AnyAlias) -> Db<'_, Self> {
        let col = col.as_table();
        Db {
            col,
            info: TableInfo {
                table: &col.table,
                inner: OnceCell::new(),
            },
        }
    }
}

impl MyIdenT for i64 {
    type Alias = MyAlias;
    type Info<'t> = ();
    fn new_alias() -> AnyAlias {
        AnyAlias::Value(MyAlias::new())
    }
    fn iden(col: &AnyAlias) -> Db<'_, Self> {
        Db {
            col: col.as_val(),
            info: (),
        }
    }
}

impl MyIdenT for bool {
    type Alias = MyAlias;
    type Info<'t> = ();
    fn new_alias() -> AnyAlias {
        AnyAlias::Value(MyAlias::new())
    }
    fn iden(col: &AnyAlias) -> Db<'_, Self> {
        Db {
            col: col.as_val(),
            info: (),
        }
    }
}

impl MyIdenT for String {
    type Alias = MyAlias;
    type Info<'t> = ();
    fn new_alias() -> AnyAlias {
        AnyAlias::Value(MyAlias::new())
    }
    fn iden(col: &AnyAlias) -> Db<'_, Self> {
        Db {
            col: col.as_val(),
            info: (),
        }
    }
}

pub struct Db<'t, T: MyIdenT> {
    pub(super) col: &'t T::Alias,
    pub(super) info: T::Info<'t>,
}

impl<'t, T: MyIdenT> Clone for Db<'t, T>
where
    T::Info<'t>: Clone,
{
    fn clone(&self) -> Self {
        Self {
            col: self.col,
            info: self.info.clone(),
        }
    }
}
impl<'t, T: MyIdenT> Copy for Db<'t, T> where T::Info<'t>: Copy {}

impl<'a, T: Table> Db<'a, T> {
    pub fn id(&self) -> Db<'a, i64> {
        self.col.val.iden()
    }
}

impl<'a, T: Table> Deref for Db<'a, T> {
    type Target = T::Dummy<'a>;

    fn deref(&self) -> &Self::Target {
        self.info
            .inner
            .get_or_init(|| T::build(Builder::new(self.info.table)))
    }
}
