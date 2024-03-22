use std::{
    cell::OnceCell,
    marker::PhantomData,
    ops::Deref,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

use elsa::FrozenVec;
use sea_query::{Alias, Expr, Iden, SimpleExpr};

use crate::{
    ast::{MyTable, Source},
    Table,
};

pub trait Value: Copy {
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

impl<'t, A: Value, B: Value> Value for (A, B) {
    fn into_expr(self) -> SimpleExpr {
        Expr::tuple([self.0.into_expr(), self.1.into_expr()]).into()
    }
}

#[derive(Clone, Copy)]
pub struct MyIden<'t> {
    pub(super) col: &'t MyAlias,
    // pub(super) _t: PhantomData<&'t ()>,
}

impl<'t> Value for MyIden<'t> {
    fn into_expr(self) -> SimpleExpr {
        Expr::col(self.col.into_alias()).into()
    }
}

#[derive(Clone, Copy)]
pub struct MyAdd<A, B>(A, B);

impl<'t, A: Value, B: Value> Value for MyAdd<A, B> {
    fn into_expr(self) -> SimpleExpr {
        self.0.into_expr().add(self.1.into_expr())
    }
}

#[derive(Clone, Copy)]
pub struct MyNot<T>(T);

impl<'t, T: Value> Value for MyNot<T> {
    fn into_expr(self) -> SimpleExpr {
        self.0.into_expr().not()
    }
}

#[derive(Clone, Copy)]
pub struct MyLt<A>(A, i32);

impl<'t, A: Value> Value for MyLt<A> {
    fn into_expr(self) -> SimpleExpr {
        Expr::expr(self.0.into_expr()).lt(self.1)
    }
}

#[derive(Clone, Copy)]
pub struct MyEq<A, B>(A, B);

impl<'t, A: Value, B: Value> Value for MyEq<A, B> {
    fn into_expr(self) -> SimpleExpr {
        self.0.into_expr().eq(self.1.into_expr())
    }
}

#[derive(Clone, Copy)]
pub struct Const<T>(pub T);

impl<'t, T> Value for Const<T>
where
    T: Into<sea_query::value::Value> + Copy,
{
    fn into_expr(self) -> SimpleExpr {
        SimpleExpr::from(self.0)
    }
}

// #[derive(Clone, Copy)]
pub struct MyAlias {
    name: u64,
    join: OnceCell<MyTable>,
}

impl MyAlias {
    pub fn new() -> Self {
        static IDEN_NUM: AtomicU64 = AtomicU64::new(0);
        let next = IDEN_NUM.fetch_add(1, Ordering::Relaxed);
        Self {
            name: next,
            join: OnceCell::new(),
        }
    }

    pub fn iden<'t>(&'t self) -> MyIden<'t> {
        MyIden {
            col: self,
            // _t: PhantomData,
        }
    }

    pub fn into_alias(&self) -> Alias {
        Alias::new(format!("{}", self.name))
    }
}

impl<'a> MyIden<'a> {
    pub fn fk<T: Table>(self) -> MyFk<'a, T> {
        MyFk {
            id: self,
            inner: OnceCell::new(),
        }
    }
}

pub struct MyFk<'a, T: Table> {
    pub id: MyIden<'a>,
    inner: OnceCell<T::Dummy<'a>>,
}

impl<'a, T: Table> Deref for MyFk<'a, T> {
    type Target = T::Dummy<'a>;

    fn deref(&self) -> &Self::Target {
        self.inner.get_or_init(|| {
            let t = self.id.col.join.get_or_init(|| MyTable {
                table: T::NAME,
                columns: FrozenVec::new(),
            });

            T::build(|name| {
                if let Some(item) = t.columns.iter().find(|item| item.0 == name) {
                    item.1.iden()
                } else {
                    let item = t.columns.push_get(Box::new((name, MyAlias::new())));
                    item.1.iden()
                }
            })
        })
    }
}
