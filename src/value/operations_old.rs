use std::{marker::PhantomData, ops::Deref};

use ref_cast::RefCast;
use sea_query::{Expr, SimpleExpr};

use super::{ExtraMethods, Value, ValueBuilder};

pub struct BinOp<T, A, B> {
    _p: PhantomData<T>,
    op: sea_query::BinOper,
    lhs: A,
    rhs: B,
}

impl<T, A: Clone, B: Clone> Clone for BinOp<T, A, B> {
    fn clone(&self) -> Self {
        Self {
            _p: self._p.clone(),
            op: self.op.clone(),
            lhs: self.lhs.clone(),
            rhs: self.rhs.clone(),
        }
    }
}
impl<T, A: Copy, B: Copy> Copy for BinOp<T, A, B> {}

impl<'t, T, A: Value<'t>, B: Value<'t>> Value<'t> for BinOp<T, A, B> {
    type Typ = T;

    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.lhs
            .build_expr(b)
            .binary(self.op, self.rhs.build_expr(b))
    }
}

impl<T: ExtraMethods, A, B> Deref for BinOp<T, A, B> {
    type Target = T::Dummy<Self>;

    fn deref(&self) -> &Self::Target {
        RefCast::ref_cast(self)
    }
}

impl<T, A, B> BinOp<T, A, B> {
    pub fn new(op: sea_query::BinOper, lhs: A, rhs: B) -> Self {
        Self {
            _p: PhantomData,
            op,
            lhs,
            rhs,
        }
    }
}

pub struct UnOp<T, A> {
    _p: PhantomData<T>,
    op: sea_query::UnOper,
    val: A,
}

impl<T, A: Clone> Clone for UnOp<T, A> {
    fn clone(&self) -> Self {
        Self {
            _p: self._p.clone(),
            op: self.op.clone(),
            val: self.val.clone(),
        }
    }
}
impl<T, A: Copy> Copy for UnOp<T, A> {}

impl<'t, T, A: Value<'t>> Value<'t> for UnOp<T, A> {
    type Typ = T;

    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        match self.op {
            sea_query::UnOper::Not => self.val.build_expr(b).not(),
        }
    }
}

impl<T: ExtraMethods, A> Deref for UnOp<T, A> {
    type Target = T::Dummy<Self>;

    fn deref(&self) -> &Self::Target {
        RefCast::ref_cast(self)
    }
}

impl<T, A> UnOp<T, A> {
    pub fn new(op: sea_query::UnOper, val: A) -> Self {
        Self {
            _p: PhantomData,
            op,
            val,
        }
    }
}

#[derive(Clone, Copy)]
pub struct UnwrapOr<A, B>(pub(crate) A, pub(crate) B);

impl<'t, A: Value<'t>, B: Value<'t>> Value<'t> for UnwrapOr<A, B> {
    type Typ = B::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).if_null(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct IsNotNull<A>(pub(crate) A);

impl<'t, A: Value<'t>> Value<'t> for IsNotNull<A> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).is_not_null()
    }
}

#[derive(Clone, Copy)]
pub struct Assume<A>(pub(crate) A);

impl<'t, T, A: Value<'t, Typ = Option<T>>> Value<'t> for Assume<A> {
    type Typ = T;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b)
    }
}
