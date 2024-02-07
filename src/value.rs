use std::{
    marker::PhantomData,
    sync::atomic::{AtomicU64, Ordering},
};

use sea_query::{Expr, Iden, SimpleExpr};

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
    pub(super) name: MyAlias,
    pub(super) _t: PhantomData<&'t ()>,
}

impl<'t> Value for MyIden<'t> {
    fn into_expr(self) -> SimpleExpr {
        Expr::col(self.name).into()
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

#[derive(Clone, Copy)]
pub struct MyAlias(u64);
impl MyAlias {
    pub fn new() -> Self {
        static IDEN_NUM: AtomicU64 = AtomicU64::new(0);
        let next = IDEN_NUM.fetch_add(1, Ordering::Relaxed);
        Self(next)
    }

    pub fn iden<'t>(self) -> MyIden<'t> {
        MyIden {
            name: self,
            _t: PhantomData,
        }
    }
}

impl Iden for MyAlias {
    fn unquoted(&self, s: &mut dyn std::fmt::Write) {
        write!(s, "{}", self.0).unwrap()
    }
}
